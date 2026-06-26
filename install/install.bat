@echo off
setlocal enabledelayedexpansion

set VERSION=1.3.0
set INSTALL_DIR=%USERPROFILE%\.local\bin
set SHARE_DIR=%USERPROFILE%\.local\share\eltr
set PROJECT_DIR=%~dp0..

if /I "%1"=="--uninstall" goto uninstall
if /I "%1"=="--help" goto usage
if /I "%1"=="-h" goto usage

echo === Elitra Language Installer v%VERSION% (Windows) ===

where cargo >nul 2>nul
if %ERRORLEVEL% neq 0 (
    echo Error: Rust/Cargo not found. Install it from https://rustup.rs
    exit /b 1
)

if exist "%INSTALL_DIR%\eltr.exe" (
    for /f "tokens=*" %%a in ('"%INSTALL_DIR%\eltr.exe" --version 2^>nul') do set OLD_VER=%%a
    if "!OLD_VER!"=="" set OLD_VER=unknown
    echo Existing installation detected: !OLD_VER!
    echo Upgrading to v%VERSION%...
)

if not exist "%INSTALL_DIR%" mkdir "%INSTALL_DIR%"
if not exist "%SHARE_DIR%\examples" mkdir "%SHARE_DIR%\examples"

echo Building Elitra v%VERSION% in release mode...
cargo build --release --manifest-path "%PROJECT_DIR%\Cargo.toml"
if %ERRORLEVEL% neq 0 (
    echo Error: Build failed
    exit /b 1
)

echo Installing binary to %INSTALL_DIR%...
copy /Y "%PROJECT_DIR%\target\release\eltr.exe" "%INSTALL_DIR%\eltr.exe" >nul

echo Installing examples...
copy /Y "%PROJECT_DIR%\examples\*.eltr" "%SHARE_DIR%\examples\" >nul 2>nul

echo Adding %INSTALL_DIR% to PATH...
setx PATH "%PATH%;%INSTALL_DIR%" >nul
if %ERRORLEVEL% equ 0 (
    echo PATH updated. You may need to restart your terminal.
) else (
    echo Warning: Could not update PATH automatically.
    echo Add %INSTALL_DIR% to your PATH manually.
)

echo.
echo Elitra Lang v%VERSION% installed successfully!
echo   eltr ^<file^>    Run a script
echo   eltr             REPL mode
echo   eltr fmt         Format code
echo   eltr test        Run tests
echo   eltr lsp         Start LSP server
echo   eltr init        Create a new project
echo   eltr run         Run project from package.toml
echo   eltr install     Install a package
echo Examples: %SHARE_DIR%\examples\
echo.
echo NOTE: You may need to restart your terminal for PATH changes to take effect.
exit /b 0

:uninstall
echo Uninstalling eltr...
if exist "%INSTALL_DIR%\eltr.exe" del "%INSTALL_DIR%\eltr.exe"
if exist "%SHARE_DIR%" rmdir /S /Q "%SHARE_DIR%"
echo Done. eltr removed.
exit /b 0

:usage
echo Usage: %0 [--uninstall]
echo   --uninstall    Remove eltr installation
exit /b 0
