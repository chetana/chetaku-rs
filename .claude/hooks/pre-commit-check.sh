#!/bin/bash
# Block git commits if cargo check finds compilation errors

INPUT=$(cat)
COMMAND=$(echo "$INPUT" | jq -r '.tool_input.command // empty')

# Only intercept git commit commands
if ! echo "$COMMAND" | grep -q "git commit"; then
  exit 0
fi

echo "Running cargo check before commit..." >&2

cd "$CLAUDE_PROJECT_DIR" || exit 0

if ! cargo check 2>&1; then
  echo "cargo check failed. Fix errors before committing." >&2
  exit 2
fi

echo "cargo check passed." >&2
exit 0
