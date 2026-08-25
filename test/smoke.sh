#!/usr/bin/env bash
# Cluaiz Automated API Smoke Test Suite (Bash)
# Usage: ./test/smoke.sh [optional_port]

PORT=${1:-8000}
BASE_URL="http://127.0.0.1:${PORT}"
PASSED=0
FAILED=0

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

echo -e "\033[0;36m🧪 Running Cluaiz API Smoke Tests against ${BASE_URL}...\033[0m"

# 1. Test Server Health / Models List
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${BASE_URL}/v1/models")
if [ "$HTTP_CODE" -eq 200 ]; then
    assert_test "GET /v1/models returns 200 list" 0 ""
else
    assert_test "GET /v1/models returns 200 list" 1 "HTTP ${HTTP_CODE}"
fi

# 2. Test max_tokens: 0 rejection (OpenAI Spec compliance)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" -X POST "${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"messages": [{"role": "user", "content": "Hello"}], "max_tokens": 0}')
if [ "$HTTP_CODE" -eq 400 ]; then
    assert_test "POST /v1/chat/completions (max_tokens: 0) returns 400" 0 ""
else
    assert_test "POST /v1/chat/completions (max_tokens: 0) returns 400" 1 "Expected 400, got ${HTTP_CODE}"
fi

# 3. Test Chat Completion Handling (Model Slot Validation)
RESP=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
    -H "Content-Type: application/json" \
    -d '{"messages": [{"role": "user", "content": "Hello"}], "max_tokens": 10}')
if echo "$RESP" | grep -E -q '(choices|model_not_found)'; then
    assert_test "POST /v1/chat/completions handles request gracefully" 0 ""
else
    assert_test "POST /v1/chat/completions handles request gracefully" 1 "$RESP"
fi

# 4. Test think_mode parameter parsing
for mode in "on" "On" "auto" "Auto"; do
    RESP=$(curl -s -X POST "${BASE_URL}/v1/chat/completions" \
        -H "Content-Type: application/json" \
        -d "{\"messages\": [{\"role\": \"user\", \"content\": \"OK\"}], \"think_mode\": \"$mode\", \"max_tokens\": 5}")
    if echo "$RESP" | grep -E -q '(choices|model_not_found)'; then
        assert_test "POST /v1/chat/completions (think_mode: '$mode')" 0 ""
    else
        assert_test "POST /v1/chat/completions (think_mode: '$mode')" 1 "$RESP"
    fi
done

echo ""
echo "📊 Smoke Test Results: $PASSED Passed, $FAILED Failed"
if [ "$FAILED" -gt 0 ]; then
    exit 1
fi
