@echo off
setlocal
cd /d "%~dp0"
where cl >nul 2>&1
if errorlevel 1 (
  echo Open a Visual Studio Developer Command Prompt first ^(cl.exe not found^).
  exit /b 1
)
echo Building tls_seed_base.dll with the static CRT...
cl /nologo /O2 /MT /DTLS_SIZE=0x2000 /c tls_seed.cpp /Fo:tls_seed.obj
if errorlevel 1 exit /b 1
link /nologo /DLL /OUT:tls_seed_base.dll tls_seed.obj
if errorlevel 1 exit /b 1
del tls_seed.obj
echo Done: tls_seed_base.dll