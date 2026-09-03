/* fstest.c — arquivos POSIX em C sobre o nexo.fs: open(O_CREAT)/write/lseek/read/close com os
 * encoders GERADOS do IDL (abi/c/proto/fs.h). Handle 0 = canal do fs (fornecido no spawn).
 * Asserts derrubam o processo com codigos distintos. */
#include "../../abi/c/nexo.h"
#include "../../sdk/libc/include/fcntl.h"
#include "../../sdk/libc/include/string.h"
#include "../../sdk/libc/include/unistd.h"

void _start(uint64_t arg) {
    (void)arg;
    nexo_libc_use_fs(0);
    int fd = open("/c-arquivo.txt", O_CREAT | O_RDWR | O_TRUNC);
    if (fd < 0)
        nexo_exit(2);
    static const char dados[] = "escrito pelo C via nexo.fs";
    if (write(fd, dados, sizeof(dados) - 1) != (ssize_t)(sizeof(dados) - 1))
        nexo_exit(3);
    if (lseek(fd, 0, SEEK_SET) != 0)
        nexo_exit(4);
    char volta[64];
    ssize_t n = read(fd, volta, sizeof(volta));
    if (n != (ssize_t)(sizeof(dados) - 1) || memcmp(volta, dados, (size_t)n) != 0)
        nexo_exit(5);
    if (lseek(fd, 8, SEEK_SET) != 8)
        nexo_exit(6);
    if (read(fd, volta, 4) != 4 || memcmp(volta, "pelo", 4) != 0)
        nexo_exit(7);
    if (close(fd) != 0 || close(fd) == 0)
        nexo_exit(8);
    static const char msg[] = "arquivos em C ok — open/write/lseek/read/close sobre nexo.fs";
    nexo_log(msg, sizeof(msg) - 1);
    nexo_exit(0);
}
