@echo off
setlocal
set "PS_SCRIPT=%~dp0agy-zh-manage.ps1"
if not exist "%PS_SCRIPT%" (
  echo [ERROR] Missing agy-zh-manage.ps1 next to agy-zh-manage.cmd
  if "%~1"=="" pause
  exit /b 1
)
powershell.exe -NoProfile -ExecutionPolicy Bypass -File "%PS_SCRIPT%" %*
set "EXIT_CODE=%ERRORLEVEL%"
if "%~1"=="" (
  echo.
  pause
)
exit /b %EXIT_CODE%
