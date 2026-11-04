@echo off
chcp 65001 >nul
cd /d "%~dp0.."
node scripts\export-code.mjs
echo.
pause
exit /b 0
