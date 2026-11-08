# TLS seed base DLL

`seed/tls_seed_base.dll` is a real MSVC-built DLL with a 0x2000-byte static-TLS template. It is the
base image for the dynamic manual-map TLS seed:

1. The injector appends a `.tlsp` section sized exactly to the payload's TLS template.
2. It patches the TLS directory `Start`/`End`/`SizeOfZeroFill` and the entry point to `mov eax,1; ret`
   (the base CRT never runs and there are no TLS callbacks).
3. The loader assigns a genuine `_tls_index` and fills a block of exactly the payload's size for
   every existing and future thread.

A synthetic PE cannot be used here: the Windows loader accepts the module but ignores its TLS
directory for per-thread allocation (verified with a probe). A real, linker-produced DLL is required.

## Why the binary is checked in

`src/inject/manual_map.rs` embeds it with `include_bytes!`. Keeping the built DLL in the repository
means building BlackBox does not require a C++ toolchain. The source and a rebuild script are
included so the binary is reproducible and auditable.

## Rebuild

From a Visual Studio Developer Command Prompt:

```cmd
cd seed
build.cmd
```

This runs:

```cmd
cl /nologo /O2 /MT /DTLS_SIZE=0x2000 /c tls_seed.cpp
link /nologo /DLL /OUT:tls_seed_base.dll tls_seed.obj
```

`/MT` links the static CRT, so the seed has no Visual C++ redistributable dependency.

## Committed binary

- Size: 111,616 bytes
- SHA-256: `7088EE278DCBB2757D36E8CDAE62661D7525F893D62DCC364B85B190BFA49D1A`
- TLS raw length: `0x2010` (`0x2000` template plus the CRT's TLS slots)

A rebuild from the same source/settings produces the same size and TLS layout, but the PE timestamp
changes the file hash. That is expected; any valid build of this source works. Do not replace the
binary with a synthetic PE.
