@echo off
rem 바둑 게임방 실행 (윈도우). 이 파일을 더블클릭하세요. 끝낼 때는 화면의 "앱 종료" 또는 이 창을 닫으세요.
chcp 65001 >nul
cd /d "%~dp0"
echo 바둑 게임방을 시작합니다...
start "" /b GoArcade.exe --urls http://localhost:5000
timeout /t 4 /nobreak >nul
start "" http://localhost:5000
echo 브라우저에서 http://localhost:5000 이 열립니다. 이 창을 닫으면 앱이 종료됩니다.
pause >nul
taskkill /im GoArcade.exe /f >nul 2>&1
