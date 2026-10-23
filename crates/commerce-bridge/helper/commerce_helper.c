/*
 * commerce_helper.exe: runs under Wine and hosts Blizzard's official Windows
 * commerce DLLs for the Linux commerce bridge (crates/commerce-bridge).
 *
 * Usage: commerce_helper.exe <dll directory> <CA bundle file>
 *
 * Reads calls from stdin and writes results and events to stdout; see
 * src/protocol.rs for the wire format. Diagnostics go to stderr.
 *
 * Every SWIG function takes and returns only integers, pointers and strings
 * (no floating point), so any of them can be called through one generic
 * signature: on Windows x64 the first four integer arguments travel in
 * registers and extra ones are harmless.
 *
 * Read-only for now: purchase calls are refused.
 *
 * Build: x86_64-w64-mingw32-gcc -O2 -o commerce_helper.exe commerce_helper.c
 */
#include <windows.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef uint64_t (*generic_fn)(uint64_t, uint64_t, uint64_t, uint64_t,
                               uint64_t, uint64_t, uint64_t, uint64_t);

enum { MAX_ARGS = 8 };
enum { ARG_INT = 0, ARG_STR = 1, ARG_LISTENER = 2, ARG_LOG = 3 };
enum { MSG_CALL = 1, MSG_EVENT_DONE = 2, MSG_RESULT = 1, MSG_EVENT = 2 };

static HANDLE in, out;
static HMODULE libraries[2];
static char *ca_bundle;
static DWORD rpc_thread;

static const char *const refused[] = {
    "CSharp_BlizzardfCommerce_blz_checkout_purchase___",
    "CSharp_BlizzardfCommerce_blz_checkout_battlenet_purchase___",
    "CSharp_BlizzardfCommerce_blz_commerce_vc_purchase___",
};

static void read_exact(void *buffer, DWORD len) {
    char *at = buffer;
    while (len) {
        DWORD got = 0;
        if (!ReadFile(in, at, len, &got, NULL) || got == 0)
            exit(0); /* the game closed the pipe */
        at += got;
        len -= got;
    }
}

static void write_all(const void *buffer, DWORD len) {
    const char *at = buffer;
    while (len) {
        DWORD put = 0;
        if (!WriteFile(out, at, len, &put, NULL))
            exit(1);
        at += put;
        len -= put;
    }
}

static uint8_t read_u8(void) { uint8_t v; read_exact(&v, 1); return v; }
static uint16_t read_u16(void) { uint16_t v; read_exact(&v, 2); return v; }
static uint32_t read_u32(void) { uint32_t v; read_exact(&v, 4); return v; }
static uint64_t read_u64(void) { uint64_t v; read_exact(&v, 8); return v; }

/* A string argument. Never freed: the SDK may keep the pointer. */
static char *read_string(void) {
    if (!read_u8())
        return NULL;
    uint32_t len = read_u32();
    char *text = malloc(len + 1);
    read_exact(text, len);
    text[len] = 0;
    return text;
}

static void serve(int until_event_done);

static void __cdecl listener(uint64_t owner, uint64_t event) {
    if (GetCurrentThreadId() != rpc_thread) {
        fprintf(stderr, "[commerce-helper] event %p from another thread dropped\n", (void *)event);
        return;
    }
    uint8_t message[17] = {MSG_EVENT};
    memcpy(message + 1, &owner, 8);
    memcpy(message + 9, &event, 8);
    write_all(message, sizeof message);
    serve(1); /* the game may call back into the SDK before it is done */
}

static void __cdecl log_hook(void *owner, int level, const char *subsystem, const char *text) {
    static const char *names[] = {"noise", "debug", "info", "warning", "error", "fatal"};
    fprintf(stderr, "[commerce %s] %s: %s\n", level >= 0 && level < 6 ? names[level] : "?",
            subsystem ? subsystem : "", text ? text : "");
}

static char *__cdecl string_callback(const char *text) { return _strdup(text ? text : ""); }

static uint64_t refused_result(void) {
    FARPROC create = GetProcAddress(libraries[0], "CSharp_BlizzardfCommerce_new_blz_commerce_result_t___");
    FARPROC set = GetProcAddress(libraries[0], "CSharp_BlizzardfCommerce_blz_commerce_result_t_state_set___");
    if (!create || !set)
        return 0;
    uint64_t result = ((uint64_t (*)(void))create)();
    ((void (*)(uint64_t, int))set)(result, 1 /* BLZ_COMMERCE_RESULT_FAIL */);
    return result;
}

static void handle_call(void) {
    uint8_t library = read_u8();
    uint16_t name_len = read_u16();
    char name[512] = {0};
    if (name_len >= sizeof name)
        exit(1);
    read_exact(name, name_len);
    uint8_t returns_string = read_u8();
    uint8_t argc = read_u8();
    if (argc > MAX_ARGS || library > 1)
        exit(1);

    uint64_t args[MAX_ARGS] = {0};
    for (int i = 0; i < argc; i++) {
        switch (read_u8()) {
        case ARG_INT: args[i] = read_u64(); break;
        case ARG_STR: args[i] = (uint64_t)read_string(); break;
        case ARG_LISTENER: args[i] = (uint64_t)listener; break;
        case ARG_LOG: args[i] = (uint64_t)log_hook; break;
        default: exit(1);
        }
    }

    uint64_t result = 0;
    int refuse = 0;
    for (size_t i = 0; i < sizeof refused / sizeof *refused; i++)
        refuse |= strcmp(name, refused[i]) == 0;
    if (refuse) {
        fprintf(stderr, "[commerce-helper] %s refused: the bridge is read-only\n", name);
        result = refused_result();
    } else {
        if (library == 1 && strcmp(name, "SetHttpOptions") == 0)
            args[2] = (uint64_t)ca_bundle; /* a Linux path means nothing to Windows */
        FARPROC function = GetProcAddress(libraries[library], name);
        if (function)
            result = ((generic_fn)function)(args[0], args[1], args[2], args[3],
                                            args[4], args[5], args[6], args[7]);
        else
            fprintf(stderr, "[commerce-helper] missing export %s\n", name);
    }

    uint8_t header[9] = {MSG_RESULT};
    memcpy(header + 1, &result, 8);
    write_all(header, sizeof header);
    if (returns_string) {
        /* SWIG returned what string_callback allocated. */
        char *text = (char *)result;
        uint8_t present = text != NULL;
        write_all(&present, 1);
        if (text) {
            uint32_t len = (uint32_t)strlen(text);
            write_all(&len, 4);
            write_all(text, len);
            free(text);
        }
    }
}

static void serve(int until_event_done) {
    for (;;) {
        switch (read_u8()) {
        case MSG_CALL: handle_call(); break;
        case MSG_EVENT_DONE:
            if (until_event_done)
                return;
            exit(1);
        default: exit(1);
        }
    }
}

static HMODULE load(const char *dir, const char *file) {
    char path[MAX_PATH];
    snprintf(path, sizeof path, "%s\\%s", dir, file);
    HMODULE module = LoadLibraryA(path);
    if (!module)
        fprintf(stderr, "[commerce-helper] cannot load %s (error %lu)\n", path, GetLastError());
    return module;
}

int main(int argc, char **argv) {
    if (argc != 3) {
        fprintf(stderr, "usage: commerce_helper.exe <dll directory> <CA bundle file>\n");
        return 2;
    }
    in = GetStdHandle(STD_INPUT_HANDLE);
    out = GetStdHandle(STD_OUTPUT_HANDLE);
    ca_bundle = argv[2];
    rpc_thread = GetCurrentThreadId();
    SetDllDirectoryA(argv[1]);
    libraries[1] = load(argv[1], "commerce_http_client.dll");
    libraries[0] = load(argv[1], "blz_commerce_sdk_plugin.dll");
    if (!libraries[0] || !libraries[1])
        return 1;
    ((void (*)(void *))GetProcAddress(libraries[0], "SWIGRegisterStringCallback_battlenet_commerce"))(
        (void *)string_callback);
    fprintf(stderr, "[commerce-helper] ready\n");
    serve(0);
    return 0;
}
