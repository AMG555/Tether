@echo off
title Tether Setup
echo ========================================================
echo   Tether - Offline Crash Recovery and Desktop Companion
echo ========================================================
echo.
echo Installing Tether to your user profile...
powershell -NoProfile -ExecutionPolicy Bypass -Command "irm https://raw.githubusercontent.com/AMG555/Tether/main/scripts/install.ps1 | iex"
echo.
echo Setup completed! You can access Tether from your Desktop or Start Menu.
echo Global hotkey [Ctrl+Alt+T] registered.
echo.
pause
