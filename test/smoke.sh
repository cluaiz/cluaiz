#!/usr/bin/env bash
# Cluaiz Automated API Smoke Test Suite (Bash)
# Usage: 
#   ./test/smoke.sh                (Tests against currently running server on :8000)
#   ./test/smoke.sh --start        (Automatically boots cluaiz serve, tests, and shuts down)
#   ./test/smoke.sh 8080           (Custom port)

START_SERVER=false
PORT=8000

for arg in "$@"; do
    if [ "$arg" == "--start" ]; then
        START_SERVER=true
    elif [[ "$arg" =~ ^[0-9]+$ ]]; then
        PORT=$arg
    fi
done

BASE_URL="http://127.0.0.1:${PORT}"
PASSED=0
FAILED=0
SKIPPED=0
SERVER_PID=""

assert_test() {
    local name="$1"
    local status="$2"
    local details="$3"
    if [ "$status" -eq 0 ]; then
        echo -e "  \033[0;32m✅ PASS:\033[0m $name"
        PASSED=$((PASSED + 1))
    else
        echo -e "  \033[0;31m❌ FAIL:\033[0m $name ($details)"
        FAILED=$((FAILED + 1))
    fi
}

skip_test() {
    local name="$1"
    local reason="$2"
    echo -e "  \033[0;33m⚠️ SKIP:\033[0m $name ($reason)"
    SKIPPED=$((SKIPPED + 1))
}

cleanup() {
    if [ -n "$SERVER_PID" ]; then
        echo -e "\033[0;36m🛑 Stopping Cluaiz Engine Daemon (PID: $SERVER_PID)...\033[0m"
        kill -9 "$SERVER_PID" 2>/dev/null || true
    fi
}
trap cleanup EXIT

if [ "$START_SERVER" = true ]; then
    echo -e "\033[0;36m🚀 Starting Cluaiz Engine Daemon on port ${PORT}...\033[0m"
    ./target/debug/cluaiz serve &
    SERVER_PID=$!
    
    # Wait for server readiness
    READY=false
    for i in {1..30}; do
        sleep 0.5
        if curl -s "${BASE_URL}/health" >/dev/null 2>&1; then
            READY=true
            break
        fi
    done
    if [ "$READY" = false ]; then
        echo -e "\033[0;31m❌ Failed to start server within 15 seconds.\033[0m"
        exit 1
    fi
fi

echo -e "\033[0;36m🧪 Running Cluaiz API Smoke Tests against ${BASE_URL}...\033[0m"

# 1. Test Server Health Check
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/health")
if [ "$HTTP_CODE" -eq 200 ]; then
    assert_test "GET /health returns 200" 0 ""
else
    assert_test "GET /health returns 200" 1 "HTTP ${HTTP_CODE}"
fi

# 2. Test OpenAI Standard Models List
MODELS_RESP=$(curl -s "${BASE_URL}/v1/models")
if echo "$MODELS_RESP" | grep -q '"object":"list"'; then
    assert_test "GET /v1/models returns OpenAI list format" 0 ""
else
    assert_test "GET /v1/models returns OpenAI list format" 1 "$MODELS_RESP"
fi

# 3. Test max_tokens: 0 rejection (OpenAI Spec compliance)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"messages": [{"role": "user", "content": "Hello"}], "max_tokens": 0}')
if [ "$HTTP_CODE" -eq 400 ]; then
    assert_test "POST /v1/chat/completions (max_tokens: 0) returns 400" 0 ""
else
    assert_test "POST /v1/chat/completions (max_tokens: 0) returns 400" 1 "Expected 400, got ${HTTP_CODE}"
fi

# 4. Probe for Active Model
PROBE_RESP=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"messages": [{"role": "user", "content": "Test"}], "max_tokens": 1}')

if echo "$PROBE_RESP" | grep -q '"choices"'; then
    # 5. Live Inference: Temperature 0.0 Determinism Test
    RESP1=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"messages": [{"role": "user", "content": "Say 'Cluaiz' and nothing else."}], "temperature": 0.0, "max_tokens": 10}')
    RESP2=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d '{"messages": [{"role": "user", "content": "Say 'Cluaiz' and nothing else."}], "temperature": 0.0, "max_tokens": 10}')
    
    TEXT1=$(echo "$RESP1" | grep -o '"content":"[^"]*"' | head -n 1)
    TEXT2=$(echo "$RESP2" | grep -o '"content":"[^"]*"' | head -n 1)
    if [ "$TEXT1" == "$TEXT2" ] && [ -n "$TEXT1" ]; then
        assert_test "Temperature 0.0 produces deterministic output" 0 ""
    else
        assert_test "Temperature 0.0 produces deterministic output" 1 "Run1: $TEXT1 vs Run2: $TEXT2"
    fi

    # 6. Live Inference: think_mode case variations
    for mode in "on" "On" "auto" "Auto"; do
        RESP=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
            -H "Content-Type: application/json" \
            -d "{\"messages\": [{\"role\": \"user\", \"content\": \"OK\"}], \"think_mode\": \"$mode\", \"max_tokens\": 5}")
        if echo "$RESP" | grep -q '"choices"'; then
            assert_test "POST /v1/chat/completions (think_mode: '$mode')" 0 ""
        else
            assert_test "POST /v1/chat/completions (think_mode: '$mode')" 1 "$RESP"
        fi
    done
else
    skip_test "POST /v1/chat/completions (live inference)" "No active model loaded in chat_slot (Install/load a model to run live generation tests)"
fi

echo ""
echo "📊 Smoke Test Results: $PASSED Passed, $FAILED Failed, $SKIPPED Skipped"
if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
