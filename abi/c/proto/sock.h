/* nexo_proto_sock.h — protocolo tipado `nexo.sock` v1.0 em C.
 * GERADO por tools/idlgen do idl/sock.idl — nao editar. Fio identico ao Rust:
 * cabecalho NXIP de 24 bytes + payload little-endian (bytes<N>: u32 len + dados).
 * Nesta rodada: encode de PEDIDO + decode de RESPOSTA (clientes C); handles nao
 * ocupam payload — viajam no vetor de handles de nexo_channel_send/recv. */
#ifndef NEXO_PROTO_SOCK_H
#define NEXO_PROTO_SOCK_H

#include <stdint.h>
#include <stddef.h>

#define NEXO_SOCK_PROTOCOL_ID 0x60281105u
#define NEXO_SOCK_VMAJOR 1
#define NEXO_SOCK_VMINOR 0

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_sock_info_req;

typedef struct {
    uint8_t ip[4];
    uint32_t ip_len;
    uint8_t mask[4];
    uint32_t mask_len;
    uint8_t gateway[4];
    uint32_t gateway_len;
    uint8_t dns[4];
    uint32_t dns_len;
    uint8_t mac[6];
    uint32_t mac_len;
} nexo_sock_info_resp;

/* Codifica o pedido `info` (metodo 1); devolve o tamanho ou -1. */
static inline int nexo_sock_info_req_encode(uint8_t *out, size_t cap, const nexo_sock_info_req *m) {
    size_t o = 24;
    (void)m;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 1; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `info`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_info_resp_decode(const uint8_t *b, size_t len, nexo_sock_info_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->ip[i] = b[o + 4 + i];
        m->ip_len = l; o += 4 + l; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->mask[i] = b[o + 4 + i];
        m->mask_len = l; o += 4 + l; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->gateway[i] = b[o + 4 + i];
        m->gateway_len = l; o += 4 + l; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->dns[i] = b[o + 4 + i];
        m->dns_len = l; o += 4 + l; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 6 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->mac[i] = b[o + 4 + i];
        m->mac_len = l; o += 4 + l; }
    }
    return 0;
}

typedef struct {
    uint8_t name[253];
    uint32_t name_len;
} nexo_sock_resolve_req;

typedef struct {
    uint8_t addr[4];
    uint32_t addr_len;
    uint8_t cached;
} nexo_sock_resolve_resp;

/* Codifica o pedido `resolve` (metodo 2); devolve o tamanho ou -1. */
static inline int nexo_sock_resolve_req_encode(uint8_t *out, size_t cap, const nexo_sock_resolve_req *m) {
    size_t o = 24;
    if (m->name_len > 253 || o + 4 + m->name_len > cap) return -1;
    { uint32_t l = m->name_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->name_len; i++) out[o + 4 + i] = m->name[i]; }
    o += 4 + m->name_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 2; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `resolve`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_resolve_resp_decode(const uint8_t *b, size_t len, nexo_sock_resolve_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->addr[i] = b[o + 4 + i];
        m->addr_len = l; o += 4 + l; }
      { uint64_t v = 0; size_t i; if (o + 1 > len) return -1; for (i = 0; i < 1; i++) v |= (uint64_t)b[o + i] << (8 * i); m->cached = (uint8_t)v; o += 1; }
    }
    return 0;
}

typedef struct {
    uint8_t dst_ip[4];
    uint32_t dst_ip_len;
    uint16_t dst_port;
    uint16_t src_port;
    uint8_t data[1400];
    uint32_t data_len;
} nexo_sock_udp_send_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_sock_udp_send_resp;

/* Codifica o pedido `udp_send` (metodo 3); devolve o tamanho ou -1. */
static inline int nexo_sock_udp_send_req_encode(uint8_t *out, size_t cap, const nexo_sock_udp_send_req *m) {
    size_t o = 24;
    if (m->dst_ip_len > 4 || o + 4 + m->dst_ip_len > cap) return -1;
    { uint32_t l = m->dst_ip_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->dst_ip_len; i++) out[o + 4 + i] = m->dst_ip[i]; }
    o += 4 + m->dst_ip_len;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->dst_port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->src_port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (m->data_len > 1400 || o + 4 + m->data_len > cap) return -1;
    { uint32_t l = m->data_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->data_len; i++) out[o + 4 + i] = m->data[i]; }
    o += 4 + m->data_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 3; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `udp_send`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_udp_send_resp_decode(const uint8_t *b, size_t len, nexo_sock_udp_send_resp *m) {
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
    uint16_t port;
} nexo_sock_udp_recv_req;

typedef struct {
    uint8_t from_ip[4];
    uint32_t from_ip_len;
    uint16_t from_port;
    uint8_t data[1400];
    uint32_t data_len;
} nexo_sock_udp_recv_resp;

/* Codifica o pedido `udp_recv` (metodo 4); devolve o tamanho ou -1. */
static inline int nexo_sock_udp_recv_req_encode(uint8_t *out, size_t cap, const nexo_sock_udp_recv_req *m) {
    size_t o = 24;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 4; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `udp_recv`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_udp_recv_resp_decode(const uint8_t *b, size_t len, nexo_sock_udp_recv_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->from_ip[i] = b[o + 4 + i];
        m->from_ip_len = l; o += 4 + l; }
      { uint64_t v = 0; size_t i; if (o + 2 > len) return -1; for (i = 0; i < 2; i++) v |= (uint64_t)b[o + i] << (8 * i); m->from_port = (uint16_t)v; o += 2; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 1400 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->data[i] = b[o + 4 + i];
        m->data_len = l; o += 4 + l; }
    }
    return 0;
}

typedef struct {
    uint8_t dst_ip[4];
    uint32_t dst_ip_len;
    uint16_t dst_port;
} nexo_sock_tcp_connect_req;

typedef struct {
    uint32_t conn;
} nexo_sock_tcp_connect_resp;

/* Codifica o pedido `tcp_connect` (metodo 5); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_connect_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_connect_req *m) {
    size_t o = 24;
    if (m->dst_ip_len > 4 || o + 4 + m->dst_ip_len > cap) return -1;
    { uint32_t l = m->dst_ip_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->dst_ip_len; i++) out[o + 4 + i] = m->dst_ip[i]; }
    o += 4 + m->dst_ip_len;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->dst_port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 5; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_connect`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_connect_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_connect_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->conn = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint32_t conn;
    uint8_t data[1400];
    uint32_t data_len;
} nexo_sock_tcp_send_req;

typedef struct {
    uint32_t sent;
} nexo_sock_tcp_send_resp;

/* Codifica o pedido `tcp_send` (metodo 6); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_send_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_send_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->conn; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (m->data_len > 1400 || o + 4 + m->data_len > cap) return -1;
    { uint32_t l = m->data_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->data_len; i++) out[o + 4 + i] = m->data[i]; }
    o += 4 + m->data_len;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 6; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_send`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_send_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_send_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->sent = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint32_t conn;
} nexo_sock_tcp_recv_req;

typedef struct {
    uint8_t data[1400];
    uint32_t data_len;
    uint8_t closed;
} nexo_sock_tcp_recv_resp;

/* Codifica o pedido `tcp_recv` (metodo 7); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_recv_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_recv_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->conn; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 7; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_recv`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_recv_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_recv_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 1400 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->data[i] = b[o + 4 + i];
        m->data_len = l; o += 4 + l; }
      { uint64_t v = 0; size_t i; if (o + 1 > len) return -1; for (i = 0; i < 1; i++) v |= (uint64_t)b[o + i] << (8 * i); m->closed = (uint8_t)v; o += 1; }
    }
    return 0;
}

typedef struct {
    uint16_t port;
} nexo_sock_tcp_listen_req;

typedef struct {
    uint32_t conn;
    uint8_t peer_ip[4];
    uint32_t peer_ip_len;
    uint16_t peer_port;
} nexo_sock_tcp_listen_resp;

/* Codifica o pedido `tcp_listen` (metodo 9); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_listen_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_listen_req *m) {
    size_t o = 24;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 9; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_listen`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_listen_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_listen_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->conn = (uint32_t)v; o += 4; }
      { uint32_t l; size_t i;
        if (o + 4 > len) return -1;
        l = (uint32_t)b[o] | ((uint32_t)b[o + 1] << 8) | ((uint32_t)b[o + 2] << 16) | ((uint32_t)b[o + 3] << 24);
        if (l > 4 || o + 4 + l > len) return -1;
        for (i = 0; i < l; i++) m->peer_ip[i] = b[o + 4 + i];
        m->peer_ip_len = l; o += 4 + l; }
      { uint64_t v = 0; size_t i; if (o + 2 > len) return -1; for (i = 0; i < 2; i++) v |= (uint64_t)b[o + i] << (8 * i); m->peer_port = (uint16_t)v; o += 2; }
    }
    return 0;
}

typedef struct {
    uint16_t port;
} nexo_sock_udp_avail_req;

typedef struct {
    uint32_t queued;
} nexo_sock_udp_avail_resp;

/* Codifica o pedido `udp_avail` (metodo 12); devolve o tamanho ou -1. */
static inline int nexo_sock_udp_avail_req_encode(uint8_t *out, size_t cap, const nexo_sock_udp_avail_req *m) {
    size_t o = 24;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->port; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 12; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `udp_avail`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_udp_avail_resp_decode(const uint8_t *b, size_t len, nexo_sock_udp_avail_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->queued = (uint32_t)v; o += 4; }
    }
    return 0;
}

typedef struct {
    uint32_t conn;
} nexo_sock_tcp_avail_req;

typedef struct {
    uint32_t avail;
    uint8_t closed;
} nexo_sock_tcp_avail_resp;

/* Codifica o pedido `tcp_avail` (metodo 10); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_avail_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_avail_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->conn; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 10; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_avail`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_avail_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_avail_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      { uint64_t v = 0; size_t i; if (o + 4 > len) return -1; for (i = 0; i < 4; i++) v |= (uint64_t)b[o + i] << (8 * i); m->avail = (uint32_t)v; o += 4; }
      { uint64_t v = 0; size_t i; if (o + 1 > len) return -1; for (i = 0; i < 1; i++) v |= (uint64_t)b[o + i] << (8 * i); m->closed = (uint8_t)v; o += 1; }
    }
    return 0;
}

typedef struct {
    uint32_t conn;
} nexo_sock_tcp_close_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_sock_tcp_close_resp;

/* Codifica o pedido `tcp_close` (metodo 8); devolve o tamanho ou -1. */
static inline int nexo_sock_tcp_close_req_encode(uint8_t *out, size_t cap, const nexo_sock_tcp_close_req *m) {
    size_t o = 24;
    if (o + 4 > cap) return -1;
    { uint64_t v = (uint64_t)m->conn; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 4;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 8; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `tcp_close`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_tcp_close_resp_decode(const uint8_t *b, size_t len, nexo_sock_tcp_close_resp *m) {
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
    uint32_t chan; /* handle: viaja no vetor, nao no payload */
    uint8_t allow_dns;
    uint8_t allow_listen;
    uint8_t rule_ip[4];
    uint32_t rule_ip_len;
    uint8_t rule_prefix;
    uint16_t rule_port_lo;
    uint16_t rule_port_hi;
    uint8_t rule_protos;
} nexo_sock_open_req;

typedef struct {
    uint8_t _vazio; /* sem campos */
} nexo_sock_open_resp;

/* Codifica o pedido `open` (metodo 11); devolve o tamanho ou -1. */
static inline int nexo_sock_open_req_encode(uint8_t *out, size_t cap, const nexo_sock_open_req *m) {
    size_t o = 24;
    if (o + 1 > cap) return -1;
    { uint64_t v = (uint64_t)m->allow_dns; size_t i; for (i = 0; i < 1; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 1;
    if (o + 1 > cap) return -1;
    { uint64_t v = (uint64_t)m->allow_listen; size_t i; for (i = 0; i < 1; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 1;
    if (m->rule_ip_len > 4 || o + 4 + m->rule_ip_len > cap) return -1;
    { uint32_t l = m->rule_ip_len; size_t i; for (i = 0; i < 4; i++) out[o + i] = (uint8_t)(l >> (8 * i)); }
    { size_t i; for (i = 0; i < m->rule_ip_len; i++) out[o + 4 + i] = m->rule_ip[i]; }
    o += 4 + m->rule_ip_len;
    if (o + 1 > cap) return -1;
    { uint64_t v = (uint64_t)m->rule_prefix; size_t i; for (i = 0; i < 1; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 1;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->rule_port_lo; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (o + 2 > cap) return -1;
    { uint64_t v = (uint64_t)m->rule_port_hi; size_t i; for (i = 0; i < 2; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 2;
    if (o + 1 > cap) return -1;
    { uint64_t v = (uint64_t)m->rule_protos; size_t i; for (i = 0; i < 1; i++) out[o + i] = (uint8_t)(v >> (8 * i)); }
    o += 1;
    if (cap < 24) return -1;
    /* magic NXIP: u32 0x4e584950 em little-endian no fio */
    out[0] = 0x50; out[1] = 0x49; out[2] = 0x58; out[3] = 0x4e;
    { uint32_t v = NEXO_SOCK_PROTOCOL_ID; size_t i; for (i = 0; i < 4; i++) out[4 + i] = (uint8_t)(v >> (8 * i)); }
    out[8] = (uint8_t)NEXO_SOCK_VMAJOR; out[9] = (uint8_t)(NEXO_SOCK_VMAJOR >> 8);
    out[10] = (uint8_t)NEXO_SOCK_VMINOR; out[11] = (uint8_t)(NEXO_SOCK_VMINOR >> 8);
    { uint32_t v = 11; size_t i; for (i = 0; i < 4; i++) out[12 + i] = (uint8_t)(v >> (8 * i)); }
    out[16] = out[17] = out[18] = out[19] = 0; /* flags: pedido */
    { uint32_t v = (uint32_t)(o - 24); size_t i; for (i = 0; i < 4; i++) out[20 + i] = (uint8_t)(v >> (8 * i)); }
    return (int)o;
}

/* Decodifica a resposta de `open`: 0 = ok; >0 = erro remoto; -1 = malformada. */
static inline int nexo_sock_open_resp_decode(const uint8_t *b, size_t len, nexo_sock_open_resp *m) {
    if (len < 24 || b[0] != 0x50 || b[1] != 0x49 || b[2] != 0x58 || b[3] != 0x4e) return -1;
    { uint32_t fl = (uint32_t)b[16] | ((uint32_t)b[17] << 8) | ((uint32_t)b[18] << 16) | ((uint32_t)b[19] << 24);
      if (fl & 2u) { if (len < 28) return -1;
        return (int)((uint32_t)b[24] | ((uint32_t)b[25] << 8) | ((uint32_t)b[26] << 16) | ((uint32_t)b[27] << 24)); } }
    { size_t o = 24;
      (void)o; (void)m;
    }
    return 0;
}

#endif /* NEXO_PROTO_SOCK_H */
