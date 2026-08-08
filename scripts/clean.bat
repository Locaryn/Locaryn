@echo off
:: Locaryn clean script (Windows).
:: Removes release/, target/, node_modules, and Tauri bundle outputs.

cd /d "%~dp0.."

echo [Locaryn] Cleaning build artifacts...

if exist release rmdir /s /q release
if exist target rmdir /s /q target
if exist apps\desktop\node_modules rmdir /s /q apps\desktop\node_modules
if exist apps\desktop\dist rmdir /s /q apps\desktop\dist
if exist packages-ui\core\node_modules rmdir /s /q packages-ui\core\node_modules
if exist packages-ui\chat\node_modules rmdir /s /q packages-ui\chat\node_modules
if exist packages-ui\preview\node_modules rmdir /s /q packages-ui\preview\node_modules
if exist packages-ui\terminal\node_modules rmdir /s /q packages-ui\terminal\node_modules
if exist node_modules rmdir /s /q node_modules

echo [Locaryn] Clean complete.
