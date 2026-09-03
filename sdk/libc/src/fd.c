/* fd.c — nexo-libc: descritores POSIX de ARQUIVO sobre o protocolo tipado nexo.fs
 * (encoders gerados em abi/c/proto/fs.h — mesma fonte IDL do Rust, nunca desatualiza).
 * Tabela por processo: fd -> {ino, offset}; o canal do fs vem de nexo_libc_use_fs. */
#include "../include/dirent.h"
#include "../include/fcntl.h"
#include "../include/stdio.h"
#include "../include/unistd.h"
#include "../include/string.h"
#include "../../../abi/c/nexo.h"
#include "../../../abi/c/proto/fs.h"

#define FD_MAX 16
#define FD_BASE 3 /* 0..2: read(0) = canal de stdin no handle 2; write(1/2) = stdio em linhas */

typedef struct {
    int usado;
    uint32_t ino;
    uint64_t off;
    uint64_t tam;
} arq;

static arq tabela[FD_MAX];
static uint32_t canal_fs = 0xffffffffu;
static uint8_t msg[4400];

void nexo_libc_use_fs(unsigned int canal) { canal_fs = canal; }

static int rpc(int n) {
    /* o kernel valida os ponteiros mesmo com capacidade 0: vetor de handles sempre real */
    uint32_t hs[2];
    if (nexo_channel_send(canal_fs, msg, (uint64_t)n, hs, 0) != NEXO_STATUS_OK)
        return -1;
    uint64_t nb = 0, nh = 0;
    if (nexo_channel_recv(canal_fs, msg, sizeof(msg), hs, 2, &nb, &nh) != NEXO_STATUS_OK)
        return -1;
    return (int)nb;
}

static int caminho(const char *path, uint8_t *dst, uint32_t *len) {
    size_t n = strlen(path);
    if (n == 0 || n > 256)
        return -1;
    memcpy(dst, path, n);
    *len = (uint32_t)n;
    return 0;
}

int open(const char *path, int flags) {
    if (canal_fs == 0xffffffffu)
        return -1;
    int fd;
    for (fd = 0; fd < FD_MAX; fd++)
        if (!tabela[fd].usado)
            break;
    if (fd == FD_MAX)
        return -1;
    nexo_fs_stat_req st = {0};
    if (caminho(path, st.path, &st.path_len))
        return -1;
    int n = nexo_fs_stat_req_encode(msg, sizeof(msg), &st);
    if (n < 0 || (n = rpc(n)) < 0)
        return -1;
    nexo_fs_stat_resp str;
    int rc = nexo_fs_stat_resp_decode(msg, (size_t)n, &str);
    if (rc < 0)
        return -1;
    if (rc > 0 && (flags & O_CREAT)) {
        nexo_fs_create_req cr = {0};
        caminho(path, cr.path, &cr.path_len);
        n = nexo_fs_create_req_encode(msg, sizeof(msg), &cr);
        if (n < 0 || (n = rpc(n)) < 0)
            return -1;
        nexo_fs_create_resp crr;
        if (nexo_fs_create_resp_decode(msg, (size_t)n, &crr) != 0)
            return -1;
        str.ino = crr.ino;
        str.size = 0;
    } else if (rc != 0) {
        return -1;
    }
    if (flags & O_TRUNC) {
        nexo_fs_truncate_req tr = {str.ino, 0};
        n = nexo_fs_truncate_req_encode(msg, sizeof(msg), &tr);
        nexo_fs_truncate_resp trr;
        if (n < 0 || (n = rpc(n)) < 0 || nexo_fs_truncate_resp_decode(msg, (size_t)n, &trr) != 0)
            return -1;
        str.size = 0;
    }
    tabela[fd].usado = 1;
    tabela[fd].ino = str.ino;
    tabela[fd].off = 0;
    tabela[fd].tam = str.size;
    return fd + FD_BASE;
}

static arq *pega(int fd) {
    fd -= FD_BASE;
    if (fd < 0 || fd >= FD_MAX || !tabela[fd].usado)
        return 0;
    return &tabela[fd];
}

/* stdin (fd 0): canal no handle 2 pela convencao do Nexo — cada mensagem e um pedaco de
 * entrada; fila vazia com a outra ponta viva BLOQUEIA (recv do kernel), PeerClosed (ou
 * handle 2 inexistente) = EOF. Buffer local serve leituras menores que a mensagem. */
#define STDIN_HANDLE 2
static uint8_t ent[4096];
static size_t ent_n, ent_pos;
static int ent_eof;

static ssize_t le_stdin(void *buf, size_t n) {
    while (ent_pos == ent_n && !ent_eof) {
        uint64_t nb = 0, nh = 0;
        uint32_t hs[2];
        if (nexo_channel_recv(STDIN_HANDLE, ent, sizeof(ent), hs, 2, &nb, &nh)
            != NEXO_STATUS_OK) {
            ent_eof = 1;
            break;
        }
        ent_n = (size_t)nb;
        ent_pos = 0;
    }
    size_t tem = ent_n - ent_pos;
    if (tem == 0)
        return 0; /* EOF */
    size_t leva = tem < n ? tem : n;
    memcpy(buf, ent + ent_pos, leva);
    ent_pos += leva;
    return (ssize_t)leva;
}

ssize_t read(int fd, void *buf, size_t n) {
    if (fd == 0)
        return le_stdin(buf, n);
    arq *a = pega(fd);
    if (!a)
        return -1;
    size_t feito = 0;
    while (feito < n) {
        uint32_t quer = (uint32_t)((n - feito) > 4000 ? 4000 : (n - feito));
        nexo_fs_read_req rq = {a->ino, a->off, quer};
        int m = nexo_fs_read_req_encode(msg, sizeof(msg), &rq);
        if (m < 0 || (m = rpc(m)) < 0)
            return -1;
        nexo_fs_read_resp rr;
        if (nexo_fs_read_resp_decode(msg, (size_t)m, &rr) != 0)
            return -1;
        if (rr.data_len == 0)
            break;
        memcpy((char *)buf + feito, rr.data, rr.data_len);
        feito += rr.data_len;
        a->off += rr.data_len;
    }
    return (ssize_t)feito;
}

ssize_t write(int fd, const void *buf, size_t n) {
    if (fd == 1 || fd == 2) { /* stdout/stderr: mesmo caminho em linhas do puts/printf */
        nexo_stdio_write((const char *)buf, n);
        return (ssize_t)n;
    }
    arq *a = pega(fd);
    if (!a)
        return -1;
    size_t feito = 0;
    while (feito < n) {
        uint32_t quer = (uint32_t)((n - feito) > 3900 ? 3900 : (n - feito));
        nexo_fs_write_req wq;
        wq.ino = a->ino;
        wq.offset = a->off;
        memcpy(wq.data, (const char *)buf + feito, quer);
        wq.data_len = quer;
        int m = nexo_fs_write_req_encode(msg, sizeof(msg), &wq);
        if (m < 0 || (m = rpc(m)) < 0)
            return -1;
        nexo_fs_write_resp wr;
        if (nexo_fs_write_resp_decode(msg, (size_t)m, &wr) != 0 || wr.written != quer)
            return -1;
        feito += quer;
        a->off += quer;
        if (a->off > a->tam)
            a->tam = a->off;
    }
    return (ssize_t)feito;
}

int close(int fd) {
    arq *a = pega(fd);
    if (!a)
        return -1;
    a->usado = 0;
    return 0;
}

off_t lseek(int fd, off_t off, int whence) {
    arq *a = pega(fd);
    if (!a)
        return -1;
    off_t base = whence == SEEK_SET ? 0 : whence == SEEK_CUR ? (off_t)a->off : (off_t)a->tam;
    off_t novo = base + off;
    if (novo < 0)
        return -1;
    a->off = (uint64_t)novo;
    return novo;
}

int unlink(const char *path) {
    if (canal_fs == 0xffffffffu)
        return -1;
    nexo_fs_unlink_req uq = {0};
    if (caminho(path, uq.path, &uq.path_len))
        return -1;
    int n = nexo_fs_unlink_req_encode(msg, sizeof(msg), &uq);
    if (n < 0 || (n = rpc(n)) < 0)
        return -1;
    nexo_fs_unlink_resp ur;
    return nexo_fs_unlink_resp_decode(msg, (size_t)n, &ur) == 0 ? 0 : -1;
}

/* dirent: um list por opendir; readdir percorre [ino u32][kind u8][len u8][nome] local. */
#define DIR_MAX 2
static DIR dirs[DIR_MAX];

DIR *opendir(const char *path) {
    if (canal_fs == 0xffffffffu)
        return 0;
    DIR *d = 0;
    for (int i = 0; i < DIR_MAX; i++)
        if (!dirs[i].usado) {
            d = &dirs[i];
            break;
        }
    if (!d)
        return 0;
    nexo_fs_list_req lq = {0};
    if (caminho(path, lq.path, &lq.path_len))
        return 0;
    int n = nexo_fs_list_req_encode(msg, sizeof(msg), &lq);
    if (n < 0 || (n = rpc(n)) < 0)
        return 0;
    static nexo_fs_list_resp lr; /* ~4K: fora da pilha, um opendir por vez ja e o contrato */
    if (nexo_fs_list_resp_decode(msg, (size_t)n, &lr) != 0)
        return 0;
    d->usado = 1;
    d->pos = 0;
    d->len = lr.entries_len;
    memcpy(d->dados, lr.entries, lr.entries_len);
    return d;
}

struct dirent *readdir(DIR *d) {
    if (!d || !d->usado || d->pos + 6 > d->len)
        return 0;
    const unsigned char *p = d->dados + d->pos;
    unsigned int nlen = p[5];
    if (d->pos + 6 + nlen > d->len || nlen >= sizeof(d->ent.d_name))
        return 0;
    d->ent.d_ino = (unsigned int)p[0] | ((unsigned int)p[1] << 8) | ((unsigned int)p[2] << 16)
                   | ((unsigned int)p[3] << 24);
    d->ent.d_type = p[4];
    memcpy(d->ent.d_name, p + 6, nlen);
    d->ent.d_name[nlen] = 0;
    d->pos += 6 + nlen;
    return &d->ent;
}

int closedir(DIR *d) {
    if (!d || !d->usado)
        return -1;
    d->usado = 0;
    return 0;
}
