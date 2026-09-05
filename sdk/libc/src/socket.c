/* socket.c — nexo-libc: sockets BSD sobre o protocolo tipado nexo.sock (encoders gerados em
 * abi/c/proto/sock.h). Tabela por processo: fd -> {tipo, conexao TCP ou porta UDP, sobra de
 * recepcao}. read/write/close de fd.c delegam para ca (ganchos fracos) quando fd >= 64. */
#include "../include/sys/socket.h"
#include "../include/string.h"
#include "../../../abi/c/nexo.h"
#include "../../../abi/c/proto/sock.h"

#define SOCK_MAX 8
#define SOCK_BASE 64
#define PEDACO 1400

typedef struct {
    int usado;
    int tipo;          /* SOCK_STREAM ou SOCK_DGRAM */
    uint32_t conn;     /* TCP */
    uint16_t porta;    /* UDP: porta ligada por bind */
    int fechado;       /* TCP: o par fechou e a sobra ja foi entregue */
    uint8_t sobra[PEDACO];
    size_t sob_pos, sob_len;
} sk;

static sk tabela[SOCK_MAX];
static uint32_t canal = 0xffffffffu;
static uint8_t msg[4400];

void nexo_libc_use_sock(unsigned int c) { canal = c; }

static int rpc(int n) {
    uint32_t hs[2]; /* o kernel exige vetor real mesmo com capacidade 0 */
    if (nexo_channel_send(canal, msg, (uint64_t)n, hs, 0) != NEXO_STATUS_OK)
        return -1;
    uint64_t nb = 0, nh = 0;
    if (nexo_channel_recv(canal, msg, sizeof(msg), hs, 2, &nb, &nh) != NEXO_STATUS_OK)
        return -1;
    return (int)nb;
}

static sk *pega(int fd) {
    fd -= SOCK_BASE;
    if (fd < 0 || fd >= SOCK_MAX || !tabela[fd].usado)
        return 0;
    return &tabela[fd];
}

int socket(int domain, int type, int protocol) {
    (void)protocol;
    if (domain != AF_INET || (type != SOCK_STREAM && type != SOCK_DGRAM) || canal == 0xffffffffu)
        return -1;
    for (int i = 0; i < SOCK_MAX; i++) {
        if (!tabela[i].usado) {
            memset(&tabela[i], 0, sizeof(tabela[i]));
            tabela[i].usado = 1;
            tabela[i].tipo = type;
            return SOCK_BASE + i;
        }
    }
    return -1;
}

static void ip_de(const struct sockaddr *addr, uint8_t ip[4], uint16_t *porta) {
    const struct sockaddr_in *a = (const struct sockaddr_in *)addr;
    uint32_t v = a->sin_addr.s_addr; /* ordem de rede: byte mais significativo primeiro */
    ip[0] = (uint8_t)v;
    ip[1] = (uint8_t)(v >> 8);
    ip[2] = (uint8_t)(v >> 16);
    ip[3] = (uint8_t)(v >> 24);
    *porta = ntohs(a->sin_port);
}

int connect(int fd, const struct sockaddr *addr, socklen_t len) {
    (void)len;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_STREAM)
        return -1;
    nexo_sock_tcp_connect_req rq = {0};
    ip_de(addr, rq.dst_ip, &rq.dst_port);
    rq.dst_ip_len = 4;
    int n = nexo_sock_tcp_connect_req_encode(msg, sizeof(msg), &rq);
    if (n < 0 || (n = rpc(n)) < 0)
        return -1;
    nexo_sock_tcp_connect_resp rr;
    if (nexo_sock_tcp_connect_resp_decode(msg, (size_t)n, &rr) != 0)
        return -1;
    s->conn = rr.conn;
    return 0;
}

int bind(int fd, const struct sockaddr *addr, socklen_t len) {
    (void)len;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_DGRAM)
        return -1;
    uint8_t ip[4];
    ip_de(addr, ip, &s->porta);
    return 0;
}

ssize_t send(int fd, const void *buf, size_t n, int flags) {
    (void)flags;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_STREAM)
        return -1;
    size_t feito = 0;
    while (feito < n) {
        size_t quer = n - feito > PEDACO ? PEDACO : n - feito;
        nexo_sock_tcp_send_req rq;
        rq.conn = s->conn;
        memcpy(rq.data, (const char *)buf + feito, quer);
        rq.data_len = (uint32_t)quer;
        int m = nexo_sock_tcp_send_req_encode(msg, sizeof(msg), &rq);
        if (m < 0 || (m = rpc(m)) < 0)
            return -1;
        nexo_sock_tcp_send_resp rr;
        if (nexo_sock_tcp_send_resp_decode(msg, (size_t)m, &rr) != 0 || rr.sent != quer)
            return -1;
        feito += quer;
    }
    return (ssize_t)feito;
}

/* Bloqueante: sonda o netd ate haver dados; `closed` com fila vazia = 0 (fim). */
ssize_t recv(int fd, void *buf, size_t n, int flags) {
    (void)flags;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_STREAM)
        return -1;
    while (s->sob_pos == s->sob_len) {
        if (s->fechado)
            return 0;
        nexo_sock_tcp_recv_req rq = {s->conn};
        int m = nexo_sock_tcp_recv_req_encode(msg, sizeof(msg), &rq);
        if (m < 0 || (m = rpc(m)) < 0)
            return -1;
        nexo_sock_tcp_recv_resp rr;
        if (nexo_sock_tcp_recv_resp_decode(msg, (size_t)m, &rr) != 0)
            return -1;
        if (rr.data_len) {
            memcpy(s->sobra, rr.data, rr.data_len);
            s->sob_pos = 0;
            s->sob_len = rr.data_len;
        } else if (rr.closed) {
            s->fechado = 1; /* closed com dados ja foi tratado acima: so termina com a fila vazia */
        } else {
            nexo_sleep_ns(10000000);
        }
    }
    size_t tem = s->sob_len - s->sob_pos;
    size_t leva = tem < n ? tem : n;
    memcpy(buf, s->sobra + s->sob_pos, leva);
    s->sob_pos += leva;
    return (ssize_t)leva;
}

ssize_t sendto(int fd, const void *buf, size_t n, int flags, const struct sockaddr *to,
               socklen_t tolen) {
    (void)flags;
    (void)tolen;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_DGRAM || n > PEDACO)
        return -1;
    nexo_sock_udp_send_req rq = {0};
    ip_de(to, rq.dst_ip, &rq.dst_port);
    rq.dst_ip_len = 4;
    rq.src_port = s->porta;
    memcpy(rq.data, buf, n);
    rq.data_len = (uint32_t)n;
    int m = nexo_sock_udp_send_req_encode(msg, sizeof(msg), &rq);
    if (m < 0 || (m = rpc(m)) < 0)
        return -1;
    nexo_sock_udp_send_resp rr;
    return nexo_sock_udp_send_resp_decode(msg, (size_t)m, &rr) == 0 ? (ssize_t)n : -1;
}

ssize_t recvfrom(int fd, void *buf, size_t n, int flags, struct sockaddr *from,
                 socklen_t *fromlen) {
    (void)flags;
    sk *s = pega(fd);
    if (!s || s->tipo != SOCK_DGRAM)
        return -1;
    for (;;) {
        nexo_sock_udp_recv_req rq = {s->porta};
        int m = nexo_sock_udp_recv_req_encode(msg, sizeof(msg), &rq);
        if (m < 0 || (m = rpc(m)) < 0)
            return -1;
        nexo_sock_udp_recv_resp rr;
        if (nexo_sock_udp_recv_resp_decode(msg, (size_t)m, &rr) != 0)
            return -1;
        if (rr.data_len) {
            size_t leva = rr.data_len < n ? rr.data_len : n;
            memcpy(buf, rr.data, leva);
            if (from) {
                struct sockaddr_in *a = (struct sockaddr_in *)from;
                a->sin_family = AF_INET;
                a->sin_port = htons(rr.from_port);
                a->sin_addr.s_addr = (uint32_t)rr.from_ip[0] | ((uint32_t)rr.from_ip[1] << 8)
                                     | ((uint32_t)rr.from_ip[2] << 16)
                                     | ((uint32_t)rr.from_ip[3] << 24);
                if (fromlen)
                    *fromlen = sizeof(*a);
            }
            return (ssize_t)leva;
        }
        nexo_sleep_ns(10000000);
    }
}

int nexo_resolve(const char *name, uint8_t ip[4]) {
    if (canal == 0xffffffffu)
        return -1;
    nexo_sock_resolve_req rq = {0};
    size_t l = strlen(name);
    if (l == 0 || l > 253)
        return -1;
    memcpy(rq.name, name, l);
    rq.name_len = (uint32_t)l;
    int m = nexo_sock_resolve_req_encode(msg, sizeof(msg), &rq);
    if (m < 0 || (m = rpc(m)) < 0)
        return -1;
    nexo_sock_resolve_resp rr;
    if (nexo_sock_resolve_resp_decode(msg, (size_t)m, &rr) != 0 || rr.addr_len != 4)
        return -1;
    memcpy(ip, rr.addr, 4);
    return 0;
}

/* Ganchos chamados por fd.c (fracos la): read/write/close em fd de socket. */
ssize_t nexo_sock_read(int fd, void *buf, size_t n) { return recv(fd, buf, n, 0); }
ssize_t nexo_sock_write(int fd, const void *buf, size_t n) { return send(fd, buf, n, 0); }
int nexo_sock_close(int fd) {
    sk *s = pega(fd);
    if (!s)
        return -1;
    if (s->tipo == SOCK_STREAM) {
        nexo_sock_tcp_close_req rq = {s->conn};
        int m = nexo_sock_tcp_close_req_encode(msg, sizeof(msg), &rq);
        if (m >= 0 && (m = rpc(m)) >= 0) {
            nexo_sock_tcp_close_resp rr;
            (void)nexo_sock_tcp_close_resp_decode(msg, (size_t)m, &rr);
        }
    }
    s->usado = 0;
    return 0;
}
