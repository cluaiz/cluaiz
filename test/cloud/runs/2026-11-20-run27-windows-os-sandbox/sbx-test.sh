#!/bin/bash
# Windows AppContainer checks through the helper. Prints only READABLE/DENIED style results, never file contents.
S="<scratch>"
EXE="./target/release/cluaiz.exe"
W=$(cygpath -w "$S/sbx-ws")
export MSYS_NO_PATHCONV=1
run() { local net=$1; shift; "$EXE" --cluaiz-os-sandbox-exec $net --workspace "$W" -- "$@"; echo "[exit $?]"; }
echo "== 1 write+read inside the workspace"; run --no-net cmd.exe /c "echo hi> inside.txt && type inside.txt"
echo "== 2 write outside (home)"; run --no-net cmd.exe /c "echo x> %USERPROFILE%\sbx-probe.txt && echo WROTE || echo DENIED"
test -e "$USERPROFILE/sbx-probe.txt" && { echo "probe file exists, removing"; rm -f "$USERPROFILE/sbx-probe.txt"; } || echo "probe file absent: ok"
echo "== 3 read ~/.ssh"; run --no-net cmd.exe /c "dir %USERPROFILE%\.ssh >nul 2>&1 && echo READABLE || echo DENIED"
echo "== 4 read ~/.cluaiz"; run --no-net cmd.exe /c "dir %USERPROFILE%\.cluaiz >nul 2>&1 && echo READABLE || echo DENIED"
echo "== 5 read Documents"; run --no-net cmd.exe /c "dir %USERPROFILE%\Documents >nul 2>&1 && echo READABLE || echo DENIED"
echo "== 6 network off"; run --no-net powershell.exe -NoProfile -Command "try{(Invoke-WebRequest https://example.com -UseBasicParsing -TimeoutSec 15).StatusCode}catch{'NET-BLOCKED'}"
echo "== 7 network on"; run --net powershell.exe -NoProfile -Command "try{(Invoke-WebRequest https://example.com -UseBasicParsing -TimeoutSec 15).StatusCode}catch{'NET-FAILED: ' + \$_.Exception.Message}"
echo "== 8 exit code"; run --no-net cmd.exe /c "exit 7"
echo "== 9 tools"; for t in git node python cargo npm; do echo "-- $t"; run --no-net cmd.exe /c "$t --version"; done
echo "== 10 Program Files readable"; run --no-net cmd.exe /c "dir \"%ProgramFiles%\" >nul 2>&1 && echo READABLE || echo DENIED"
