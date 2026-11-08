// Minimal real DLL with a static-TLS template used as the base for the dynamic manual-map seed.
//
// The injector appends a `.tlsp` section sized exactly to the payload's TLS template, patches the
// TLS directory Start/End/ZeroFill and the entry point to `mov eax,1; ret`, then loads this DLL in
// the target. The Windows loader then assigns a genuine `_tls_index` and fills a block of exactly
// that size for every thread. The base DLL's own DllMain is never called (the entry point is
// replaced); it only has to be a real, linker-produced PE with a TLS directory.
//
// Rebuild with build.cmd from a Visual Studio developer prompt. The checked-in binary is kept so
// building BlackBox itself does not require a C++ toolchain.
#include <windows.h>

#ifndef TLS_SIZE
#define TLS_SIZE 0x2000
#endif

extern "C" __declspec(thread) unsigned char g_tls_seed[TLS_SIZE] = {1};

BOOL WINAPI DllMain(HINSTANCE h, DWORD reason, LPVOID reserved) {
    (void)h;
    (void)reason;
    (void)reserved;
    volatile unsigned char sink = g_tls_seed[0];
    (void)sink;
    return TRUE;
}

extern "C" __declspec(dllexport) int TlsSeedPing(void) { return 1; }