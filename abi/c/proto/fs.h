/* nexo_proto_fs.h — protocolo tipado `nexo.fs` v1.0 em C.
 * GERADO por tools/idlgen do idl/fs.idl — nao editar. Fio identico ao Rust:
 * cabecalho NXIP de 24 bytes + payload little-endian (bytes<N>: u32 len + dados).
 * Nesta rodada: encode de PEDIDO + decode de RESPOSTA (clientes C); handles nao
 * ocupam payload — viajam no vetor de handles de nexo_channel_send/recv. */
#ifndef NEXO_PROTO_FS_H
#define NEXO_PROTO_FS_H

#include <stdint.h>
#include <stddef.h>

#define NEXO_FS_PROTOCOL_ID 0x2d3847ceu
#define NEXO_FS_VMAJOR 1
#define NEXO_FS_VMINOR 0

typedef struct {
    uint8_t path[256];
    uint32_t path_len;
} nexo_fs_stat_req;

typedef struct {
    uint32_t ino;
    uint8_t kind;
    uint64_t size;
} nexo_fs_stat_resp;

/* Codifica o pedido `stat` (metodo 1); devolve o tamanho ou -1. */
static inline int nexo_fs_stat_req_encode(uint8_t *out, size_t cap, const nexo_fs_stat_req *m) {
    size_t o = 24;
    if (m->path_len > 256 || o + 4 + m->path_len > cap) return -1;
    { uint32_t l = m->path_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->path_len; i++) out[o + 4 + i] = m->path[i]; }
    o += 4 + m->path_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 1; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `stat`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_stat_resp_decode(const uint8_t *b, size_t len, nexo_fs_stat_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->ino = (uint32_t)v; o += 4; }
      { uint64_t v = 0; size_t i; if (o + 1 > len) return -1; for (i = 0; i < 1; i++) v |= (uint64_t)b[o + i] << (8 * i); m->kind = (uint8_t)v; o += 1; }
      { uint64_t v = 0; size_t i; if (o + 8 > len) return -1; for (i = 0; i < 8; i++) v |= (uint64_t)b[o + i] << (8 * i); m->size = (uint64_t)v; o += 8; }
    }
    return 0;
}

typedef struct {
    uint8_t path[256];
    uint32_t path_len;
} nexo_fs_create_req;

typedef struct {
    uint32_t ino;
} nexo_fs_create_resp;

/* Codifica o pedido `create` (metodo 2); devolve o tamanho ou -1. */
static inline int nexo_fs_create_req_encode(uint8_t *out, size_t cap, const nexo_fs_create_req *m) {
    size_t o = 24;
    if (m->path_len > 256 || o + 4 + m->path_len > cap) return -1;
    { uint32_t l = m->path_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->path_len; i++) out[o + 4 + i] = m->path[i]; }
    o += 4 + m->path_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 2; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `create`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_create_resp_decode(const uint8_t *b, size_t len, nexo_fs_create_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->ino = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint8_t path[256];
    uint32_t path_len;
} nexo_fs_mkdir_req;

typedef struct {
    uint32_t ino;
} nexo_fs_mkdir_resp;

/* Codifica o pedido `mkdir` (metodo 3); devolve o tamanho ou -1. */
static inline int nexo_fs_mkdir_req_encode(uint8_t *out, size_t cap, const nexo_fs_mkdir_req *m) {
    size_t o = 24;
    if (m->path_len > 256 || o + 4 + m->path_len > cap) return -1;
    { uint32_t l = m->path_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->path_len; i++) out[o + 4 + i] = m->path[i]; }
    o += 4 + m->path_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 3; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `mkdir`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_mkdir_resp_decode(const uint8_t *b, size_t len, nexo_fs_mkdir_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->ino = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint8_t path[256];
    uint32_t path_len;
} nexo_fs_unlink_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_fs_unlink_resp;

/* Codifica o pedido `unlink` (metodo 4); devolve o tamanho ou -1. */
static inline int nexo_fs_unlink_req_encode(uint8_t *out, size_t cap, const nexo_fs_unlink_req *m) {
    size_t o = 24;
    if (m->path_len > 256 || o + 4 + m->path_len > cap) return -1;
    { uint32_t l = m->path_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->path_len; i++) out[o + 4 + i] = m->path[i]; }
    o += 4 + m->path_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 4; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `unlink`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_unlink_resp_decode(const uint8_t *b, size_t len, nexo_fs_unlink_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      (void)o; (void)m;
    }
    return 0;
}

typedef struct {
    uint32_t ino;
    uint64_t offset;
    uint32_t len;
} nexo_fs_read_req;

typedef struct {
    uint8_t data[4000];
    uint32_t data_len;
} nexo_fs_read_resp;

/* Codifica o pedido `read` (metodo 5); devolve o tamanho ou -1. */
static inline int nexo_fs_read_req_encode(uint8_t *out, size_t cap, const nexo_fs_read_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->ino; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (o + 8 > cap) return -1;
    { uint64_t v = (uint64_t)m->offset; size_t i; for (i = 0; i < 8; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 8;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 5; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `read`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_read_resp_decode(const uint8_t *b, size_t len, nexo_fs_read_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4000 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->data[i] = b[o + 4 + i];
        m->data_len = l; o += 4 + l; }
    }
    return 0;
}

typedef struct {
    uint32_t ino;
    uint64_t offset;
    uint8_t data[3900];
    uint32_t data_len;
} nexo_fs_write_req;

typedef struct {
    uint32_t written;
} nexo_fs_write_resp;

/* Codifica o pedido `write` (metodo 6); devolve o tamanho ou -1. */
static inline int nexo_fs_write_req_encode(uint8_t *out, size_t cap, const nexo_fs_write_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->ino; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (o + 8 > cap) return -1;
    { uint64_t v = (uint64_t)m->offset; size_t i; for (i = 0; i < 8; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 8;
    if (m->data_len > 3900 || o + 4 + m->data_len > cap) return -1;
    { uint32_t l = m->data_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->data_len; i++) out[o + 4 + i] = m->data[i]; }
    o += 4 + m->data_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 6; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `write`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_write_resp_decode(const uint8_t *b, size_t len, nexo_fs_write_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->written = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint8_t path[256];
    uint32_t path_len;
} nexo_fs_list_req;

typedef struct {
    uint32_t count;
    uint8_t entries[3900];
    uint32_t entries_len;
} nexo_fs_list_resp;

/* Codifica o pedido `list` (metodo 7); devolve o tamanho ou -1. */
static inline int nexo_fs_list_req_encode(uint8_t *out, size_t cap, const nexo_fs_list_req *m) {
    size_t o = 24;
    if (m->path_len > 256 || o + 4 + m->path_len > cap) return -1;
    { uint32_t l = m->path_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->path_len; i++) out[o + 4 + i] = m->path[i]; }
    o += 4 + m->path_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 7; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `list`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_list_resp_decode(const uint8_t *b, size_t len, nexo_fs_list_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->count = (uint32_t)v; o += 4; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 3900 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->entries[i] = b[o + 4 + i];
        m->entries_len = l; o += 4 + l; }
    }
    return 0;
}

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_fs_sync_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_fs_sync_resp;

/* Codifica o pedido `sync` (metodo 8); devolve o tamanho ou -1. */
static inline int nexo_fs_sync_req_encode(uint8_t *out, size_t cap, const nexo_fs_sync_req *m) {
    size_t o = 24;
    (void)m;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 8; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `sync`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_sync_resp_decode(const uint8_t *b, size_t len, nexo_fs_sync_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      (void)o; (void)m;
    }
    return 0;
}

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_fs_info_req;

typedef struct {
    uint64_t total_blocks;
    uint64_t free_blocks;
    uint64_t repairs;
    uint64_t generation;
} nexo_fs_info_resp;

/* Codifica o pedido `info` (metodo 9); devolve o tamanho ou -1. */
static inline int nexo_fs_info_req_encode(uint8_t *out, size_t cap, const nexo_fs_info_req *m) {
    size_t o = 24;
    (void)m;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 9; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `info`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_info_resp_decode(const uint8_t *b, size_t len, nexo_fs_info_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 8 > len) return -1; for (i = 0; i < 8; i++) v |= (uint64_t)b[o + i] << (8 * i); m->total_blocks = (uint64_t)v; o += 8; }
      { uint64_t v = 0; size_t i; if (o + 8 > len) return -1; for (i = 0; i < 8; i++) v |= (uint64_t)b[o + i] << (8 * i); m->free_blocks = (uint64_t)v; o += 8; }
      { uint64_t v = 0; size_t i; if (o + 8 > len) return -1; for (i = 0; i < 8; i++) v |= (uint64_t)b[o + i] << (8 * i); m->repairs = (uint64_t)v; o += 8; }
      { uint64_t v = 0; size_t i; if (o + 8 > len) return -1; for (i = 0; i < 8; i++) v |= (uint64_t)b[o + i] << (8 * i); m->generation = (uint64_t)v; o += 8; }
    }
    return 0;
}

typedef struct {
    uint32_t ino;
    uint64_t size;
} nexo_fs_truncate_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_fs_truncate_resp;

/* Codifica o pedido `truncate` (metodo 10); devolve o tamanho ou -1. */
static inline int nexo_fs_truncate_req_encode(uint8_t *out, size_t cap, const nexo_fs_truncate_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->ino; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (o + 8 > cap) return -1;
    { uint64_t v = (uint64_t)m->size; size_t i; for (i = 0; i < 8; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 8;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_FS_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_FS_VMAJOR; out[9] = (uint8_t)(NEXO_FS_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_FS_VMINOR; out[11] = (uint8_t)(NEXO_FS_VMINOR >> 8);
    { uint32_t v = 10; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `truncate`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_fs_truncate_resp_decode(const uint8_t *b, size_t len, nexo_fs_truncate_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      (void)o; (void)m;
    }
    return 0;
}

#endif /* NEXO_PROTO_FS_H */
