#!/usr/bin/env python3
"""
Ayiou External Plugin Example (Python)

This demonstrates how to write an external plugin using JSON-RPC protocol.
The plugin communicates with Ayiou via stdin/stdout.

Usage:
1. Enable external-plugin-bridge in config.toml
2. Configure this plugin:
   [external-plugin-bridge.plugins.py-demo]
   command = "python3"  # or "python" on Windows
   args = ["-u", "./examples/external_plugin.py"]  # -u for unbuffered output

Commands:
  /ping          - Simple ping/pong test
  /echo <text>   - Echo back the text
  /time          - Show current time
  /help          - Show available commands
"""

import sys
import json
from datetime import datetime

# Debug logging to stderr (won't interfere with JSON-RPC)
def log(msg):
    sys.stderr.write(f"[py-demo] {msg}\n")
    sys.stderr.flush()

def read_request():
    """Read a JSON-RPC request from stdin"""
    try:
        line = sys.stdin.readline()
        if not line:
            return None
        return json.loads(line)
    except Exception as e:
        log(f"Read error: {e}")
        return None

def write_response(response):
    """Write a JSON-RPC response to stdout"""
    sys.stdout.write(json.dumps(response) + "\n")
    sys.stdout.flush()

def success_response(id, result):
    """Create a success response"""
    return {"jsonrpc": "2.0", "id": id, "result": result}

def error_response(id, code, message):
    """Create an error response"""
    return {"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": message}}

# =============================================================================
# Protocol Handlers
# =============================================================================

def handle_metadata(id):
    """Return plugin metadata"""
    result = {
        "name": "py-demo",
        "description": "Python Demo Plugin - Echo, Ping, Time commands",
        "version": "1.0.0",
        "author": "Ayiou Team",
        "commands": [
            {
                "name": "ping",
                "description": "Simple ping/pong test",
                "aliases": []
            },
            {
                "name": "echo",
                "description": "Echo back the given text",
                "aliases": []
            },
            {
                "name": "time",
                "description": "Show current server time",
                "aliases": ["now"]
            },
            {
                "name": "help",
                "description": "Show help message",
                "aliases": ["?"]
            }
        ]
    }
    write_response(success_response(id, result))

def handle_matches(id, params):
    """Check if this plugin should handle the message"""
    text = params.get("text", "").strip()

    # Match commands starting with /
    commands = ["/ping", "/echo", "/time", "/now", "/help", "/?"]
    matches = any(text.startswith(cmd) for cmd in commands)

    write_response(success_response(id, {"matches": matches}))

def handle_handle(id, params):
    """Process the message and return a response"""
    text = params.get("text", "").strip()
    user_id = params.get("user_id", 0)
    message_type = params.get("message_type", "private")

    log(f"Handling: '{text}' from user {user_id} ({message_type})")

    # Route to command handlers
    if text == "/ping":
        reply = "Pong! (from Python plugin)"
    elif text.startswith("/echo"):
        # Extract the text after /echo
        content = text[5:].strip()
        if content:
            reply = f"Echo: {content}"
        else:
            reply = "Usage: /echo <text>"
    elif text in ("/time", "/now"):
        now = datetime.now()
        reply = f"Current time: {now.strftime('%Y-%m-%d %H:%M:%S')}"
    elif text in ("/help", "/?"):
        reply = """Available commands:
/ping  - Pong test
/echo <text>  - Echo back text
/time  - Show current time
/help  - This message"""
    else:
        # Unknown command matched by mistake
        reply = f"Unknown command: {text}"

    result = {
        "handled": True,
        "block": True,
        "reply": reply,
        "actions": []
    }
    write_response(success_response(id, result))

def handle_lifecycle(id, params):
    """Handle lifecycle events"""
    event = params.get("event", {})

    if isinstance(event, str):
        event_type = event
    elif isinstance(event, dict):
        # Serde enum representation: {"startup": null} or {"shutdown": null}
        event_type = list(event.keys())[0] if event else "unknown"
    else:
        event_type = "unknown"

    if event_type == "startup":
        log("Plugin started!")
    elif event_type == "shutdown":
        log("Plugin shutting down...")
    elif event_type == "bot_connect":
        self_id = event.get("bot_connect", {}).get("self_id", 0)
        log(f"Bot connected: {self_id}")
    else:
        log(f"Unknown lifecycle event: {event_type}")

    write_response(success_response(id, {"ok": True}))

# =============================================================================
# Main Loop
# =============================================================================

def main():
    log("Starting Python demo plugin...")

    while True:
        req = read_request()
        if req is None:
            log("EOF received, exiting")
            break

        method = req.get("method")
        id = req.get("id")
        params = req.get("params", {})

        log(f"Received: method={method}, id={id}")

        if method == "metadata":
            handle_metadata(id)
        elif method == "matches":
            handle_matches(id, params)
        elif method == "handle":
            handle_handle(id, params)
        elif method == "lifecycle":
            handle_lifecycle(id, params)
        else:
            log(f"Unknown method: {method}")
            write_response(error_response(id, -32601, f"Method not found: {method}"))

if __name__ == "__main__":
    main()
