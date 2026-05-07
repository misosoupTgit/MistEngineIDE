@echo off
echo.%*. | findstr /C:"--build" > nul 2>&1
if %errorlevel% equ 0 (
    cmake.exe %*
) else (
    cmake.exe -DCMAKE_POLICY_VERSION_MINIMUM=3.5 %*
)
