@echo off
setlocal
set "PSModulePath="
set "PS_SCRIPT=%~dp0agy-zh-manage.ps1"
if not exist "%PS_SCRIPT%" (
  echo [ERROR] Missing agy-zh-manage.ps1 next to agy-zh-manage.cmd
  if "%~1"=="" pause
  exit /b 1
)
set "SHIM_DIR=%TEMP%"
if "%SHIM_DIR%"=="" set "SHIM_DIR=%SystemRoot%\Temp"
set "SHIM=%SHIM_DIR%\agy-zh-manage-shim-v1.cmd"
set "SHIM_TMP=%SHIM%.%RANDOM%"
if not exist "%SHIM%" (
  > "%SHIM_TMP%" (
    echo @echo off
    echo powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%%PS_SCRIPT%%" %%*
    echo set "EXIT_CODE=%%ERRORLEVEL%%"
    echo if "%%~1"=="" ^(
    echo   echo.
    echo   pause
    echo ^)
    echo exit /b %%EXIT_CODE%%
  )
  move /y "%SHIM_TMP%" "%SHIM%" >nul 2>&1
)
"%SHIM%" %*
