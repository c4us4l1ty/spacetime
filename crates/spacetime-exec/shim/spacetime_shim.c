/*
 * Narrow LD_PRELOAD determinism shim for Spacetime Block 2.
 *
 * It provides a fixed clock, deterministic getrandom/getentropy bytes, and socket denial for
 * dynamically linked local probes. It is NOT a kernel sandbox: static binaries, direct syscalls,
 * inherited descriptors, and /dev/urandom reads are outside this shim's boundary.
 */
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>
#include <sys/random.h>
#include <sys/socket.h>
#include <sys/types.h>
#include <time.h>

#define FIXED_SEED 0ULL
#define MIX_OFFSET 0x9E3779B97F4A7C15ULL
#define MIX1 0xBF58476D1CE4E5B9ULL
#define MIX2 0x94D049BB133111EBULL

static uint64_t mix64(uint64_t x) {
    x ^= x >> 30;
    x *= MIX1;
    x ^= x >> 27;
    x *= MIX2;
    x ^= x >> 31;
    return x;
}

static unsigned char rng_byte(size_t index) {
    uint64_t x = mix64(FIXED_SEED + (uint64_t)index + MIX_OFFSET);
    return (unsigned char)((x >> 24) & 0xFFU);
}

static void fill_fixed_bytes(void *buffer, size_t length) {
    unsigned char *bytes = (unsigned char *)buffer;
    for (size_t index = 0; index < length; index += 1) {
        bytes[index] = rng_byte(index);
    }
}

typedef int (*clock_gettime_fn)(clockid_t, struct timespec *);

int clock_gettime(clockid_t clk_id, struct timespec *tp) {
    if (clk_id == CLOCK_REALTIME || clk_id == CLOCK_REALTIME_COARSE ||
        clk_id == CLOCK_MONOTONIC || clk_id == CLOCK_MONOTONIC_COARSE ||
        clk_id == CLOCK_MONOTONIC_RAW || clk_id == CLOCK_BOOTTIME) {
        tp->tv_sec = 0;
        tp->tv_nsec = 0;
        return 0;
    }
    static clock_gettime_fn real_clock_gettime = NULL;
    if (real_clock_gettime == NULL) {
        real_clock_gettime = (clock_gettime_fn)dlsym(RTLD_NEXT, "clock_gettime");
    }
    if (real_clock_gettime == NULL) {
        errno = ENOSYS;
        return -1;
    }
    return real_clock_gettime(clk_id, tp);
}

ssize_t getrandom(void *buf, size_t buflen, unsigned int flags) {
    (void)flags;
    fill_fixed_bytes(buf, buflen);
    return (ssize_t)buflen;
}

int getentropy(void *buffer, size_t length) {
    if (length > 256) {
        errno = EIO;
        return -1;
    }
    fill_fixed_bytes(buffer, length);
    return 0;
}

int socket(int domain, int type, int protocol) {
    (void)domain;
    (void)type;
    (void)protocol;
    errno = EACCES;
    return -1;
}

int socketpair(int domain, int type, int protocol, int sv[2]) {
    (void)domain;
    (void)type;
    (void)protocol;
    (void)sv;
    errno = EACCES;
    return -1;
}

int connect(int fd, const struct sockaddr *addr, socklen_t len) {
    (void)fd;
    (void)addr;
    (void)len;
    errno = EACCES;
    return -1;
}
