# Changelog

All notable changes to BlackBox Injector are recorded here. The project follows semantic
versioning: MAJOR.MINOR.PATCH, where PATCH releases contain backwards-compatible fixes, MINOR
releases add backwards-compatible features, and MAJOR releases contain breaking changes.

## [1.0.2]

### Added

- Manual-map static TLS on x64 can register the mapped image directly with the loader. The
  injector builds a synthetic `LDR_DATA_TABLE_ENTRY`, takes the loader lock and calls the internal
  `LdrpHandleTlsData`. The loader assigns a real index, inserts an `LDRP_TLS_ENTRY` into
  `LdrpTlsList`, writes `AddressOfIndex`, and initializes blocks for existing threads through
  `NtSetInformationProcess(ProcessTlsInformation)`; future threads allocate a block through
  `LdrpInitializeThread` / `LdrpAllocateTls`. No image file is opened and no `LoadLibraryW` /
  `LdrLoadDll` call is made, so image-load filters such as CS2 Trusted Mode's `NtOpenFile` hook
  are not invoked. `LdrpHandleTlsData` is resolved by name from the matching ntdll PDB, downloaded
  once from the Microsoft symbol server and cached in `%TEMP%\BlackBoxInjector\symbols`
  (`LOCALAPPDATA` and the injector directory are fallbacks), so it is not tied to one Windows
  build. If the PDB is neither cached nor downloadable, the injection fails with `TLS-PDB-01`
  through the normal failure dialog; no seed fallback is attempted.

### Changed

- Removed the loader-visible TLS seed. Static TLS now has one path: the loader-registration call
  resolved from the ntdll PDB. A missing symbol cache and failed Microsoft symbol download produce
  a normal injection error instead of silently trying an unsigned seed.
- The synthetic LDR entry is not added to the PEB module list, so the payload remains unlisted.
  The trade-off is that the loader's thread-init loop does not deliver the payload's
  `DLL_THREAD_ATTACH` / `DLL_THREAD_DETACH` callbacks (or `DllMain` thread attach) for future
  threads; the TLS data block itself is allocated and valid for existing and new threads.
- A target that is no longer running reports `E2003 TARGET_NOT_RUNNING` in the normal error
  dialog, and a missing/undownloadable ntdll PDB reports `TLS-PDB-01`; neither writes a failure
  report. These are user/environment errors, not injector bugs.

## [1.0.1]

### Fixed

- Manual map static TLS no longer uses a fixed set of template sizes. For every payload the
  injector now builds a real seed DLL with a `.tlsp` section sized exactly to that payload's TLS
  template, points the TLS directory at it and lets the loader assign the index and fill a block of
  exactly that size for existing and future threads. A 196 KB template works the same as an
  8-byte one.
- Removed the unsafe x64 fallback that replaced every thread's TLS array. That fallback raced with
  a busy target's thread churn and could corrupt the heap (`0xC0000374` in ntdll) on TLS-heavy
  payloads.
- Standard and NtCreateThreadEx now capture the full 64-bit HMODULE from `LoadLibraryW` through a
  small remote stub and verify the PE at that base. A DLL that unlinks itself from the PEB is
  reported as a success with its real base instead of a false "not in the module list" error.
- Crash reports now read the Windows Error Reporting Application Error fields correctly. The exception code and fault offset are no longer taken from the module timestamp fields, so a heap corruption is reported as `0xC0000374` at its real fault offset.
- `--cli ... > log.txt` and pipes keep the caller's standard output and error handles instead of
  being replaced by the attached console.

### Changed

- One embedded TLS base seed replaces the 8 KB, 64 KB and 128 KB ladder. The binary is smaller and
  there is no template ceiling.
- The Windows version resource now reports the product as BlackBox, hackvshack.net as the company,
  and the full injector description.
- README updated for the dynamic TLS seed and the loader-method verification.
- Added the TLS seed source (`seed/tls_seed.cpp`) and rebuild script (`seed/build.cmd`); the shipped base DLL is built from them.

## [1.0.0]

### Added

- Initial release: Standard, LdrLoadDll, Manual Map, Thread Hijack, NtCreateThreadEx and APC
  injection methods.
- Manual map with PE32/PE32+ relocations, imports, forwarded exports and ApiSetMap resolution,
  section protections, header wiping, `.pdata` and `KiUserInvertedFunctionTable` registration for
  C++ exceptions.
- Loader-managed static TLS, TLS callbacks, cloaking (ErasePE, HideModule, thread hiding, PE
  scrambler), crash diagnostics with NTSTATUS and `ImageBase + RVA`, GUI and CLI with queue and
  profiles.
