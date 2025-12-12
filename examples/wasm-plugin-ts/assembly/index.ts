// Ayiou WASM Plugin - AssemblyScript Example
//
// This plugin responds to /hello command with a greeting message.

// ============================================================================
// Memory buffer for host communication
// ============================================================================

// Shared buffer for JSON data exchange (4KB)
const BUFFER_SIZE: i32 = 4096;
const buffer = memory.data(BUFFER_SIZE);

// ============================================================================
// Plugin Metadata
// ============================================================================

const META_JSON: string = `{"name":"hello-ts","description":"A hello world plugin written in TypeScript/AssemblyScript","version":"1.0.0"}`;

// ============================================================================
// Context structure (parsed from JSON)
// ============================================================================

class Context {
    text: string = "";
    raw_message: string = "";
    user_id: i64 = 0;
    group_id: i64 = 0;
    is_private: bool = false;
    is_group: bool = false;
    nickname: string = "";
}

// ============================================================================
// Simple JSON parsing helpers
// ============================================================================

function getStringValue(json: string, key: string): string {
    const keyPattern = `"${key}":"`;
    const startIdx = json.indexOf(keyPattern);
    if (startIdx < 0) return "";

    const valueStart = startIdx + keyPattern.length;
    let valueEnd = valueStart;
    let escaped = false;

    while (valueEnd < json.length) {
        const char = json.charCodeAt(valueEnd);
        if (escaped) {
            escaped = false;
        } else if (char == 0x5C) { // backslash
            escaped = true;
        } else if (char == 0x22) { // quote
            break;
        }
        valueEnd++;
    }

    return json.substring(valueStart, valueEnd);
}

function getI64Value(json: string, key: string): i64 {
    const keyPattern = `"${key}":`;
    const startIdx = json.indexOf(keyPattern);
    if (startIdx < 0) return 0;

    const valueStart = startIdx + keyPattern.length;
    let valueEnd = valueStart;

    while (valueEnd < json.length) {
        const char = json.charCodeAt(valueEnd);
        if (char < 0x30 || char > 0x39) break; // not a digit
        valueEnd++;
    }

    if (valueEnd == valueStart) return 0;
    return I64.parseInt(json.substring(valueStart, valueEnd));
}

function getBoolValue(json: string, key: string): bool {
    const keyPattern = `"${key}":`;
    const startIdx = json.indexOf(keyPattern);
    if (startIdx < 0) return false;

    const valueStart = startIdx + keyPattern.length;
    return json.substr(valueStart, 4) == "true";
}

function parseContext(json: string): Context {
    const ctx = new Context();
    ctx.text = getStringValue(json, "text");
    ctx.raw_message = getStringValue(json, "raw_message");
    ctx.user_id = getI64Value(json, "user_id");
    ctx.group_id = getI64Value(json, "group_id");
    ctx.is_private = getBoolValue(json, "is_private");
    ctx.is_group = getBoolValue(json, "is_group");
    ctx.nickname = getStringValue(json, "nickname");
    return ctx;
}

// ============================================================================
// Buffer read/write helpers
// ============================================================================

function readStringFromBuffer(ptr: i32, len: i32): string {
    // Skip 4-byte length prefix, read actual string
    const strPtr = ptr + 4;
    return String.UTF8.decodeUnsafe(strPtr, len, false);
}

function writeStringToBuffer(s: string): i32 {
    const encoded = String.UTF8.encode(s, false);
    const len = encoded.byteLength;

    // Write length prefix (4 bytes, little-endian)
    store<u32>(buffer, len);

    // Write string data
    memory.copy(buffer + 4, changetype<usize>(encoded), len);

    return buffer;
}

// ============================================================================
// Exported functions (Ayiou Plugin ABI)
// ============================================================================

/**
 * Allocate memory for host to write data
 */
export function ayiou_alloc(size: i32): i32 {
    return buffer;
}

/**
 * Free memory (no-op for static buffer)
 */
export function ayiou_free(ptr: i32): void {
    // No-op: we use a static buffer
}

/**
 * Return plugin metadata as JSON
 */
export function ayiou_meta(): i32 {
    return writeStringToBuffer(META_JSON);
}

/**
 * Check if this plugin should handle the message
 */
export function ayiou_matches(ctx_ptr: i32, ctx_len: i32): i32 {
    const json = readStringFromBuffer(ctx_ptr, ctx_len);
    const ctx = parseContext(json);

    // Match /hello or /hi commands
    const text = ctx.text.trim();
    if (text.startsWith("/hello") || text.startsWith("/hi")) {
        return 1; // matches
    }
    return 0; // doesn't match
}

/**
 * Handle the message and return response
 */
export function ayiou_handle(ctx_ptr: i32, ctx_len: i32): i32 {
    const json = readStringFromBuffer(ctx_ptr, ctx_len);
    const ctx = parseContext(json);

    const text = ctx.text.trim();
    let reply: string;

    if (text.startsWith("/hello")) {
        // Extract name argument if present
        const args = text.substring(6).trim();
        if (args.length > 0) {
            reply = `Hello, ${args}! 👋`;
        } else {
            reply = `Hello, ${ctx.nickname}! 👋`;
        }
    } else if (text.startsWith("/hi")) {
        reply = `Hi there, ${ctx.nickname}! 🎉`;
    } else {
        reply = "Unknown command";
    }

    // Build response JSON
    const response = `{"block":true,"reply":"${reply}"}`;
    return writeStringToBuffer(response);
}
