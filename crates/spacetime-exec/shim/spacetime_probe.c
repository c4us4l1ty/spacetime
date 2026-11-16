/*
 * Local probe for the Spacetime LD_PRELOAD shim. It prints a single machine-checkable line.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <locale.h>
#include <stdio.h>
#include <string.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <time.h>

int main(void) {
    struct timespec ts;
    if (clock_gettime(CLOCK_REALTIME, &ts) != 0) {
        return 2;
    }

    unsigned char bytes[16];
    errno = 0;
    ssize_t got = getrandom(bytes, sizeof(bytes), 0);
    if (got != (ssize_t)sizeof(bytes)) {
        return 3;
    }

    errno = 0;
    int sock = socket(AF_INET, SOCK_STREAM, 0);
    int sock_errno = errno;

    struct sockaddr_storage address;
    memset(&address, 0, sizeof(address));
    address.ss_family = AF_INET;
    errno = 0;
    int connection = connect(-1, (const struct sockaddr *)&address,
                             (socklen_t)sizeof(address));
    int connect_errno = errno;

    const char *locale = setlocale(LC_ALL, "");
    if (locale == NULL) {
        return 4;
    }

    printf("clock=%lld.%09ld rng=%zd rng_hex=", (long long)ts.tv_sec, ts.tv_nsec, got);
    for (size_t index = 0; index < sizeof(bytes); index += 1) {
        printf("%02x", (unsigned)bytes[index]);
    }
    printf(" socket=%d socket_errno=%d connect=%d connect_errno=%d locale=%s\n",
           sock, sock_errno, connection, connect_errno, locale);
    return 0;
}
