/* sys/socket.h — nexo-libc minima: sockets BSD sobre o protocolo tipado nexo.sock do netd
 * (encoders GERADOS em abi/c/proto/sock.h — o mesmo IDL do Rust). O runtime precisa do canal
 * do netd: nexo_libc_use_sock(handle). AF_INET + SOCK_STREAM (TCP: connect/send/recv, tambem
 * read/write/close) e SOCK_DGRAM (UDP: bind/sendto/recvfrom). Descritores de socket vivem a
 * partir de 64 (os de arquivo a partir de 3). recv/recvfrom bloqueiam (sondagem com sleep). */
#ifndef NEXO_LIBC_SYS_SOCKET_H
#define NEXO_LIBC_SYS_SOCKET_H

#include <stddef.h>
#include <stdint.h>
#include "../unistd.h"

#ifdef __cplusplus
extern "C" {
#endif
#define AF_INET 2
#define SOCK_STREAM 1
#define SOCK_DGRAM 2

typedef uint32_t socklen_t;

struct in_addr {
    uint32_t s_addr; /* ordem de rede */
};

struct sockaddr_in {
    uint16_t sin_family;
    uint16_t sin_port; /* ordem de rede */
    struct in_addr sin_addr;
};

struct sockaddr {
    uint16_t sa_family;
    uint8_t sa_data[14];
};

static inline uint16_t htons(uint16_t v) { return (uint16_t)((v << 8) | (v >> 8)); }
static inline uint16_t ntohs(uint16_t v) { return htons(v); }
static inline uint32_t htonl(uint32_t v) {
    return (v << 24) | ((v & 0xff00u) << 8) | ((v >> 8) & 0xff00u) | (v >> 24);
}
static inline uint32_t ntohl(uint32_t v) { return htonl(v); }

void nexo_libc_use_sock(unsigned int canal);
int socket(int domain, int type, int protocol);
int connect(int fd, const struct sockaddr *addr, socklen_t len);
int bind(int fd, const struct sockaddr *addr, socklen_t len);
ssize_t send(int fd, const void *buf, size_t n, int flags);
ssize_t recv(int fd, void *buf, size_t n, int flags);
ssize_t sendto(int fd, const void *buf, size_t n, int flags, const struct sockaddr *to,
               socklen_t tolen);
ssize_t recvfrom(int fd, void *buf, size_t n, int flags, struct sockaddr *from,
                 socklen_t *fromlen);
/* Resolve `name` por DNS (o cache do netd); grava 4 bytes em `ip`. Devolve 0 ou -1. */
int nexo_resolve(const char *name, uint8_t ip[4]);

#ifdef __cplusplus
}
#endif

#endif
