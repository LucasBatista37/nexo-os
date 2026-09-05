/* fetch.c — cliente HTTP/1.0 minimo em C sobre os sockets da nexo-libc (sys/socket.h ->
 * nexo.sock do netd): GET <caminho>, imprime o corpo (write(1)) e sai 0 se a resposta foi
 * 200. Uso: fetch <ip> <porta> <caminho>  (canal do netd no handle 0; argv via crt0). */
#include "../../sdk/libc/include/stdio.h"
#include "../../sdk/libc/include/stdlib.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/sys/socket.h"
#include "../../sdk/libc/include/unistd.h"

static int ip_parse(const char *s, uint8_t ip[4]) {
    for (int i = 0; i < 4; i++) {
        int v = 0, d = 0;
        while (*s >= '0' && *s <= '9') {
            v = v * 10 + (*s++ - '0');
            d++;
        }
        if (!d || v > 255 || (i < 3 && *s++ != '.'))
            return -1;
        ip[i] = (uint8_t)v;
    }
    return *s == 0 ? 0 : -1;
}

int main(int argc, char **argv) {
    if (argc < 4) {
        puts("uso: fetch <ip> <porta> <caminho>");
        return 2;
    }
    uint8_t ip[4];
    if (ip_parse(argv[1], ip) != 0) {
        puts("fetch: ip invalido");
        return 2;
    }
    nexo_libc_use_sock(0);
    int fd = socket(AF_INET, SOCK_STREAM, 0);
    if (fd < 0) {
        puts("fetch: socket");
        return 1;
    }
    struct sockaddr_in a;
    a.sin_family = AF_INET;
    a.sin_port = htons((uint16_t)atoi(argv[2]));
    a.sin_addr.s_addr = (uint32_t)ip[0] | ((uint32_t)ip[1] << 8) | ((uint32_t)ip[2] << 16)
                        | ((uint32_t)ip[3] << 24);
    if (connect(fd, (struct sockaddr *)&a, sizeof(a)) != 0) {
        puts("fetch: connect");
        return 1;
    }
    char req[512];
    int n = snprintf(req, sizeof(req), "GET %s HTTP/1.0\r\nHost: %s\r\n\r\n", argv[3], argv[1]);
    if (send(fd, req, (size_t)n, 0) != n) {
        puts("fetch: send");
        return 1;
    }
    static char resp[65536];
    size_t total = 0;
    ssize_t r;
    while ((r = recv(fd, resp + total, sizeof(resp) - 1 - total, 0)) > 0) {
        total += (size_t)r;
        if (total >= sizeof(resp) - 1)
            break;
    }
    close(fd);
    resp[total] = 0;
    char *corpo = strstr(resp, "\r\n\r\n");
    if (!corpo) {
        puts("fetch: resposta sem cabecalho");
        return 1;
    }
    corpo += 4;
    write(1, corpo, total - (size_t)(corpo - resp));
    return strncmp(resp, "HTTP/1.0 200", 12) == 0 ? 0 : 1;
}
