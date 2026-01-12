# Local Model Execution Strategy

## Overview

As an alternative to using Claude sub-agents, well-structured plans can be executed using local open-source models. This approach leverages the detailed code snippets and specifications in plans to enable cheaper, faster execution on local hardware.

---

## Why Local Execution Works

Your plan structure already provides:
1. **Exact code snippets** to insert
2. **Specific file paths** and line numbers
3. **Clear acceptance criteria** that are verifiable
4. **Verification commands** to confirm completion

This is essentially "fill in the blank" programming - the hardest part (architecture, design, test strategy) is done by Opus during planning.

---

## Hardware Requirements

### NVIDIA 5090 (32GB VRAM) Capabilities

| Model | Size | VRAM Usage | Speed (tok/s) | Quality |
|-------|------|------------|---------------|---------|
| Qwen2.5-Coder-32B | 32B | ~20GB (Q4) | ~40-60 | Excellent for code |
| DeepSeek-Coder-V2-Lite | 16B | ~12GB | ~80-100 | Very good |
| CodeLlama-34B | 34B | ~22GB (Q4) | ~30-50 | Good |
| Qwen2.5-Coder-7B | 7B | ~6GB | ~150+ | Good for simple tasks |

**Recommendation:** Qwen2.5-Coder-32B-Instruct (Q4_K_M quantization) - best code quality that fits 32GB VRAM.

---

## Integration Options

### Option 1: Claude Code Router (Recommended)

Claude Code can route to local models via OpenAI-compatible API:

```json
{
  "modelRouter": {
    "rules": [
      {
        "match": { "taskPhase": "setup" },
        "model": "local:qwen2.5-coder-32b"
      },
      {
        "match": { "taskPhase": "red" },
        "model": "local:qwen2.5-coder-32b"
      },
      {
        "match": { "taskPhase": "green" },
        "model": "local:qwen2.5-coder-32b"
      },
      {
        "match": { "taskPhase": "verify" },
        "model": "claude-opus-4-5-20251101"
      }
    ]
  },
  "localModels": {
    "qwen2.5-coder-32b": {
      "endpoint": "http://localhost:11434/v1",
      "apiKey": "ollama"
    }
  }
}
```

### Option 2: OpenCode with Local Model

[OpenCode](https://github.com/opencode-ai/opencode) is designed for local model execution:

```bash
# Install
go install github.com/opencode-ai/opencode@latest

# Configure for local model
opencode config set provider ollama
opencode config set model qwen2.5-coder:32b

# Run task
opencode run --task .claude/tasks/happy-sauteeing-moon/005.md
```

### Option 3: Aider with Local Model

```bash
# Configure aider for local model
aider --model ollama/qwen2.5-coder:32b \
      --edit-format diff \
      --auto-commits \
      crates/server/src/routes/message_queue.rs
```

---

## Setup Instructions

### 1. Install Ollama

```bash
curl -fsSL https://ollama.com/install.sh | sh
```

### 2. Pull Qwen2.5-Coder-32B

```bash
# Q4_K_M quantization for 32GB VRAM
ollama pull qwen2.5-coder:32b
```

### 3. Verify Installation

```bash
# Check model is available
ollama list

# Test generation speed
time ollama run qwen2.5-coder:32b "Write a Rust function that adds two numbers"

# Start server (if not already running)
ollama serve
```

### 4. Test API Endpoint

```bash
curl http://localhost:11434/v1/chat/completions \
  -H "Content-Type: application/json" \
  -d '{
    "model": "qwen2.5-coder:32b",
    "messages": [{"role": "user", "content": "Hello"}]
  }'
```

---

## Task File Format for Local Execution

### Enhanced Frontmatter

```yaml
---
name: Add test module structure
status: open
model: local                    # NEW: local | haiku | sonnet | opus
local_model: qwen2.5-coder:32b  # Specific model for local execution
complexity: XS
tdd_phase: setup
---
```

### Simplified Prompt for Local Models

Local models work better with simpler, more direct prompts:

```markdown
# Task: Add test module structure to message_queue.rs

## File to modify
crates/server/src/routes/message_queue.rs

## Code to add at line 280
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use db::test_utils::create_test_pool;
}
```

## Verification
Run: cargo check -p server

## Done when
- [ ] Test module exists
- [ ] cargo check passes
```bash

---

## Hybrid Execution Architecture

```text
┌─────────────────────────────────────────────────────────────────┐
│ @plan (Opus - Cloud)                                            │
│ - Architecture decisions require strong reasoning               │
│ - Writes detailed code snippets into plan                       │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @start (Opus - Cloud)                                           │
│ - Task decomposition requires judgment                          │
│ - Creates task files with model: local                          │
└─────────────────────────────────────────────────────────────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @next (Sonnet - Cloud) - Orchestrator                           │
│ - Reads batches.md                                              │
│ - Spawns local model for each task                              │
│ - Validates results                                             │
│ - Escalates to Opus if local fails                              │
└─────────────────────────────────────────────────────────────────┘
          │              │              │              │
          ↓              ↓              ↓              ↓
     ┌─────────┐   ┌─────────┐   ┌─────────┐   ┌─────────┐
     │ Task 1  │   │ Task 2  │   │ Task 3  │   │ Task N  │
     │ LOCAL   │   │ LOCAL   │   │ LOCAL   │   │ LOCAL   │
     │ ~40t/s  │   │ ~40t/s  │   │ ~40t/s  │   │ ~40t/s  │
     │ $0      │   │ $0      │   │ $0      │   │ $0      │
     └─────────┘   └─────────┘   └─────────┘   └─────────┘
                              ↓
┌─────────────────────────────────────────────────────────────────┐
│ @validate (Opus - Cloud)                                        │
│ - Critical review requires strong reasoning                     │
│ - Catches any local model errors                                │
└─────────────────────────────────────────────────────────────────┘
```

---

## Modified @next Template for Local Execution

```markdown
## STEP 2: EXECUTE TASK

### Determine Execution Target

Read task frontmatter to determine executor:

| model field | Executor |
|-------------|----------|
| `local` | Local model via Ollama |
| `haiku` | Claude Haiku API |
| `sonnet` | Claude Sonnet API |
| `opus` | Claude Opus API |

### For Local Execution:

```bash
# Ensure Ollama is running with model loaded
curl -s http://localhost:11434/api/tags | jq '.models[].name' | grep qwen2.5-coder

# Execute via OpenCode or direct API call
opencode run \
  --model qwen2.5-coder:32b \
  --task-file .claude/tasks/{plan}/{task}.md \
  --working-dir /var/tmp/vkswarm/{worktree} \
  --output-format json
```
```javascript

---

## Fallback Strategy

```javascript
async function executeWithFallback(task) {
  const models = ['local', 'sonnet', 'opus'];

  for (const model of models) {
    try {
      const result = await executeTask(task, model);
      if (result.status === 'complete') {
        return result;
      }
    } catch (e) {
      console.log(`${model} failed, trying next...`);
    }
  }

  return { status: 'blocked', reason: 'All models failed' };
}
```

**Escalation order:**
1. Local model (free, fast)
2. Sonnet (cheap, reliable)
3. Opus (expensive, most capable)
4. Mark as blocker (requires human intervention)

---

## Cost Comparison

For a 9-task plan like `reject_if_remote`:

| Strategy | Cloud Cost | Local Cost | Total |
|----------|------------|------------|-------|
| All Opus | ~$15-20 | $0 | ~$18 |
| Opus + Sonnet/Haiku | ~$6-8 | $0 | ~$7 |
| Opus + Local (90%) | ~$3-4 | ~$0.50 electricity | ~$4 |

**Savings with local execution: 75-80% vs all-Opus**

---

## When to Use Local vs Cloud

### Use Local Model For:
- `setup` phase tasks (scaffolding, boilerplate)
- `red` phase tasks (writing tests from spec)
- `green` phase tasks (implementing from detailed spec)
- `refactor` phase tasks (following patterns)
- Simple documentation updates

### Use Cloud (Opus) For:
- Planning and architecture decisions
- Validation and code review
- Complex debugging
- Tasks that fail on local model
- Ambiguous requirements

---

## Performance Expectations

### Qwen2.5-Coder-32B on 5090

| Metric | Value |
|--------|-------|
| Tokens/second | 40-60 |
| Context window | 32K tokens |
| Typical task completion | 30-60 seconds |
| VRAM usage | ~20GB (Q4_K_M) |

### Comparison to Cloud

| Model | Speed | Cost | Quality |
|-------|-------|------|---------|
| Local (Qwen 32B) | ~50 tok/s | $0 | 85-90% of Sonnet |
| Haiku | ~100 tok/s | $0.25/M in | 80% of Sonnet |
| Sonnet | ~80 tok/s | $3/M in | Baseline |
| Opus | ~40 tok/s | $15/M in | 110% of Sonnet |

---

## Recommended Model Selection by Task

| Task Type | Recommended | Fallback |
|-----------|-------------|----------|
| Add test module | local | haiku |
| Write test case | local | sonnet |
| Implement function | local | sonnet |
| Integrate into handler | local | sonnet |
| Run linter/formatter | local | haiku |
| Update documentation | local | sonnet |
| Validate implementation | opus | - |
| Debug failures | opus | - |
