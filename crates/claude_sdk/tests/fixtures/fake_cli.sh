#!/bin/sh
# Fake Claude CLI that emits NDJSON to stdout.
# Reads messages from stdin (--input-format stream-json) and uses the last
# user message content as the "prompt" to select behaviour.

# Collect all args for inspection, escaping JSON-unsafe characters.
# Replace newlines with \n literal, then escape backslashes and quotes.
ARGS=$(printf '%s' "$*" | tr '\n' ' ' | sed 's/\\/\\\\/g; s/"/\\"/g')

# Read all stdin lines, store them, and extract the content of the last message.
PROMPT=""
STDIN_LINES=""
while IFS= read -r line; do
    [ -z "$line" ] && continue
    if [ -n "$STDIN_LINES" ]; then
        STDIN_LINES="$STDIN_LINES|$line"
    else
        STDIN_LINES="$line"
    fi
    # Extract content field value from the message JSON.
    CONTENT=$(echo "$line" | sed -n 's/.*"content":"\([^"]*\)".*/\1/p')
    if [ -n "$CONTENT" ]; then
        PROMPT="$CONTENT"
    fi
done

case "$PROMPT" in
    "__test_stream__")
        echo '{"type":"system","subtype":"init","session_id":"test-session-1"}'
        echo '{"type":"assistant","message":{"content":[{"type":"text","text":"Hello from fake CLI"}]}}'
        echo '{"type":"result","subtype":"success","result":"Done.","is_error":false}'
        ;;
    "__test_thinking__")
        echo '{"type":"system","subtype":"init","session_id":"test-session-2"}'
        echo '{"type":"assistant","message":{"content":[{"type":"thinking","thinking":"Let me think..."},{"type":"text","text":"Answer"}]}}'
        echo '{"type":"result","subtype":"success","result":"Done."}'
        ;;
    "__test_skip_unknown__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo '{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"123","content":"ok"}]}}'
        echo '{"type":"assistant","message":{"content":[{"type":"text","text":"after unknown"}]}}'
        echo '{"type":"result","subtype":"success","result":"ok"}'
        ;;
    "__test_empty_lines__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo ''
        echo '   '
        echo '{"type":"assistant","message":{"content":[{"type":"text","text":"survived blanks"}]}}'
        echo '{"type":"result","subtype":"success","result":"ok"}'
        ;;
    "__test_echo_args__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"ARGS: $ARGS\"}"
        ;;
    "__test_echo_env__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"API_KEY: ${ANTHROPIC_API_KEY:-unset} OAUTH_TOKEN: ${CLAUDE_CODE_OAUTH_TOKEN:-unset}\"}"
        ;;
    "__test_echo_args_env__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"ARGS: $ARGS MAX_THINKING_TOKENS: ${MAX_THINKING_TOKENS:-unset}\"}"
        ;;
    "__test_echo_stdin__")
        # Echo back the raw stdin lines (pipe-separated) so tests can verify message format
        ESCAPED_STDIN=$(echo "$STDIN_LINES" | sed 's/\\/\\\\/g; s/"/\\"/g')
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        echo "{\"type\":\"result\",\"subtype\":\"success\",\"result\":\"STDIN: $ESCAPED_STDIN\"}"
        ;;
    "__test_hang__")
        echo '{"type":"system","subtype":"init","session_id":"s1"}'
        sleep 60
        ;;
    *)
        echo '{"type":"system","subtype":"init","session_id":"default"}'
        echo '{"type":"result","subtype":"success","result":"unknown prompt"}'
        ;;
esac
