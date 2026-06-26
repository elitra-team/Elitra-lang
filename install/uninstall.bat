@echo off
setlocal enabledelayedexpansion

set INSTALL_DIR=%USERPROFILE%\.local\bin
set SHARE_DIR=%USERPROFILE%\.local\share\eltr

if /I "%1"=="--help" goto usage
if /I "%1"=="-h" goto usage

echo Uninstalling eltr...

if exist "%INSTALL_DIR%\eltr.exe" (
    del "%INSTALL_DIR%\eltr.exe"
    echo   Removed %INSTALL_DIR%\eltr.exe
) else (
    echo   %INSTALL_DIR%\eltr.exe not found
)

if exist "%SHARE_DIR%" (
    rmdir /S /Q "%SHARE_DIR%"
    echo   Removed %SHARE_DIR%
) else (
    echo   %SHARE_DIR% not found
)

echo.
echo eltr has been uninstalled.
echo To remove the source code, delete the project directory manually.
exit /b 0

:usage
echo Usage: %0
echo   (no options needed)
echo   Uninstalls eltr from %%USERPROFILE%%\.local\bin
exit /b 0
