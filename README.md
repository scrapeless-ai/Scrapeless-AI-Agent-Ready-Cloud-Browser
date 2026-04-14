
[<img width="4800" height="2516" alt="img_v3_0210o_3aad8381-32d9-4f35-9745-15c16232794g" src="https://github.com/user-attachments/assets/0b63923c-a90d-42c5-b09f-d7f07ca7de33" />](https://www.scrapeless.com/en/product/scraping-browser)


The ultimate cloud browser automation CLI purpose-built for AI agents, powered by Scrapeless. Designed to seamlessly bridge the gap between Large Language Models (LLMs) and the web, it provides headless browser control with built-in residential proxies, advanced anti-detection mechanisms, and intelligent session management.

## Features

- AI-Native Element References: Simplifies DOM interaction for AI agents using intuitive references (@e1, @e2, etc.), drastically reducing token usage and improving LLM comprehension.

- Cloud-Based Browser Automation: Execute browser tasks entirely in the cloud—no local Chrome/Chromium installation required, making it perfect for lightweight agent deployments.

- Built-In Residential Proxy Support: Seamlessly bypass geo-restrictions and IP bans with integrated residential proxies featuring precise geo-targeting.

- Automatic Browser Fingerprinting: Enterprise-grade anti-detection capabilities ensure your AI agents remain undetected while scraping or automating tasks.

- Intelligent Session Persistence: Automatic session management allows agents to maintain state, cookies, and context across complex, multi-step workflows.

- Visual Data Extraction: Built-in screenshot capabilities and structured data extraction tailored for multimodal AI models.

- Multi-Tab & Window Support: Handle complex browsing scenarios just like a human user.

- Session Recording: Essential debugging tools for monitoring and refining AI agent behavior.


## Installation

### Global Installation (recommended)

```bash
npm install -g scrapeless-scraping-browser
```

After global installation, use the `scrapeless-scraping-browser` command directly:

```bash
scrapeless-scraping-browser open example.com
```

### Quick Start (no install)

Use npx to run without installation:

```bash
npx scrapeless-scraping-browser open example.com
```

### Project Installation

```bash
npm install scrapeless-scraping-browser
```

Then use via npx:

```bash
npx scrapeless-scraping-browser open example.com
```

## Authentication

Get your API key from the [Scrapeless Dashboard](https://app.scrapeless.com), then set it using the config command or environment variable:

```bash
# Method 1: Using config command (recommended, persistent)
scrapeless-scraping-browser config set apiKey your_api_key_here

# Method 2: Using environment variable
export SCRAPELESS_API_KEY=your_api_key_here

# Or add to your shell profile for persistence
echo 'export SCRAPELESS_API_KEY=your_api_key_here' >> ~/.zshrc
```

**Priority**: Config file > Environment variable

## Quick Start for AI Workflows

**Recommended Workflow**: Create a session first, then use the session ID for all operations to maintain context for your AI agent:

```bash
# Step 1: Set your API key
export SCRAPELESS_API_KEY=your_token

# Step 2: Create a session and save the session ID
SESSION_ID=$(scrapeless-scraping-browser new-session --name "my-workflow" --ttl 1800 --json | jq -r '.taskId')

# Step 3: Use the session ID for all browser operations
scrapeless-scraping-browser --session-id $SESSION_ID open https://example.com
scrapeless-scraping-browser --session-id $SESSION_ID snapshot -i
scrapeless-scraping-browser --session-id $SESSION_ID click @e1
scrapeless-scraping-browser --session-id $SESSION_ID fill @e2 "test@example.com"
scrapeless-scraping-browser --session-id $SESSION_ID screenshot page.png

# Step 4: Close the session when done
scrapeless-scraping-browser --session-id $SESSION_ID close
```

**Why use --session-id?**
- Ensures all commands use the same browser session, critical for maintaining state in AI workflows.
- Prevents automatic session creation/switching which can confuse agent logic.
- Provides explicit control over session lifecycle.
- Required for reliable multi-step workflows.

**Note**: If you don't specify `--session-id`, the CLI will automatically:
1. Look for the latest running session and use it
2. If no running session exists, create a new one
3. For production workflows, always use `--session-id` for consistency

**Important**: The session ID returned by `new-session` is Scrapeless's session taskId. Use this taskId as the `--session-id` parameter for all subsequent browser operations.

## Session Management

### Creating Sessions

```bash
# Create a basic session
scrapeless-scraping-browser new-session

# Create a named session with custom timeout
scrapeless-scraping-browser new-session --name "test-session" --ttl 300

# Create session with Australian proxy
scrapeless-scraping-browser new-session --proxy-country AU --proxy-city sydney

# Create session with custom browser configuration
scrapeless-scraping-browser new-session --platform macOS --screen-width 1440 --screen-height 900

# Create session with multiple languages
scrapeless-scraping-browser new-session --languages "en,es,fr" --timezone "Europe/Madrid"

# Advanced session with all options
scrapeless-scraping-browser new-session \
  --name "advanced-session" \
  --ttl 7200 \
  --recording true \
  --proxy-country US \
  --proxy-state CA \
  --proxy-city "Los Angeles" \
  --platform iOS \
  --screen-width 375 \
  --screen-height 812 \
  --user-agent "Mozilla/5.0 (iPhone; CPU iPhone OS 15_0 like Mac OS X)" \
  --timezone "America/Los_Angeles" \
  --languages "en,es"
```

**Available Session Options:**
- `--name <name>`: Session name for identification
- `--ttl <seconds>`: Session timeout in seconds (default: 180)
- `--recording <true|false>`: Enable session recording for debugging
- `--proxy-country <code>`: Proxy country code (AU, US, GB, CN, JP, etc.)
- `--proxy-state <state>`: Proxy state/region (NSW, CA, NY, TX, etc.)
- `--proxy-city <city>`: Proxy city (sydney, newyork, london, tokyo, etc.)
- `--user-agent <ua>`: Custom user agent string
- `--platform <platform>`: Platform (Windows, macOS, Linux, iOS, Android)
- `--screen-width <px>`: Screen width in pixels (default: 1920)
- `--screen-height <px>`: Screen height in pixels (default: 1080)
- `--timezone <tz>`: Timezone (default: America/New_York)
- `--languages <langs>`: Comma-separated language codes (default: en)

### Managing Sessions

```bash
# List all running sessions
scrapeless-scraping-browser sessions

# Use specific session for operations (recommended)
scrapeless-scraping-browser --session-id abc123 open example.com
scrapeless-scraping-browser --session-id abc123 snapshot -i
scrapeless-scraping-browser --session-id abc123 click @e1

# Get live preview URL for current or specific session
scrapeless-scraping-browser live
scrapeless-scraping-browser live abc123

# Stop specific session
scrapeless-scraping-browser stop abc123

# Stop all running sessions
scrapeless-scraping-browser stop-all
```

**Important**: All browser operation commands support the `--session-id` parameter to specify which session to use. This is the recommended way to ensure all operations use the same browser session.

## Core Commands

**Note**: All browser operation commands support the optional `--session-id <id>` parameter to specify which Scrapeless session to use.

### Navigation & Session Management

```bash
scrapeless-scraping-browser new-session [options]              # Create new browser session
scrapeless-scraping-browser [--session-id <id>] open <url>      # Navigate to URL
scrapeless-scraping-browser [--session-id <id>] close           # Close browser session
scrapeless-scraping-browser sessions                           # List running sessions
scrapeless-scraping-browser live [taskId]                      # Get live preview URL
scrapeless-scraping-browser stop <taskId>                      # Stop specific session
scrapeless-scraping-browser stop-all                           # Stop all sessions
```

### Page Interaction

```bash
scrapeless-scraping-browser [--session-id <id>] snapshot -i             # Get interactive elements with refs
scrapeless-scraping-browser [--session-id <id>] click @e1               # Click element by ref
scrapeless-scraping-browser [--session-id <id>] fill @e2 "text"         # Fill input field
scrapeless-scraping-browser [--session-id <id>] type @e2 "text"         # Type into element
scrapeless-scraping-browser [--session-id <id>] press Enter             # Press keyboard key
scrapeless-scraping-browser [--session-id <id>] scroll down 500         # Scroll page
scrapeless-scraping-browser [--session-id <id>] wait @e1                # Wait for element
scrapeless-scraping-browser [--session-id <id>] wait --load networkidle # Wait for network idle
```

### Get Information

```bash
scrapeless-scraping-browser [--session-id <id>] get text @e1            # Get element text
scrapeless-scraping-browser [--session-id <id>] get url                 # Get current URL
scrapeless-scraping-browser [--session-id <id>] get title               # Get page title
scrapeless-scraping-browser [--session-id <id>] screenshot              # Take screenshot
scrapeless-scraping-browser [--session-id <id>] screenshot --full       # Full page screenshot
```

### Cookies & Storage

```bash
scrapeless-scraping-browser [--session-id <id>] cookies                 # Get all cookies
scrapeless-scraping-browser [--session-id <id>] cookies set <name> <val> # Set cookie
scrapeless-scraping-browser [--session-id <id>] cookies clear           # Clear cookies
scrapeless-scraping-browser [--session-id <id>] storage local           # Get localStorage
scrapeless-scraping-browser [--session-id <id>] storage local set <k> <v>  # Set localStorage
```

### Multi-Page Management

```bash
scrapeless-scraping-browser [--session-id <id>] pages                   # List all pages/tabs
scrapeless-scraping-browser [--session-id <id>] page <pageId>           # Switch to specific page
scrapeless-scraping-browser [--session-id <id>] tab new [url]           # Open new tab
scrapeless-scraping-browser [--session-id <id>] tab close [n]           # Close tab
```

## Configuration

All configuration can be managed via the `config` command or environment variable (API key only):

```bash
# Set API key (Method 1: Config file - recommended, persistent)
scrapeless-scraping-browser config set apiKey your_api_key

# Set API key (Method 2: Environment variable)
export SCRAPELESS_API_KEY=your_api_key

# Set proxy country
scrapeless-scraping-browser config set proxyCountry US

# Set session timeout (in seconds)
scrapeless-scraping-browser config set sessionTtl 300

# Set session name
scrapeless-scraping-browser config set sessionName my-session

# List all configuration
scrapeless-scraping-browser config list

# Get specific value
scrapeless-scraping-browser config get apiKey

# Remove configuration
scrapeless-scraping-browser config remove proxyCountry
```

Configuration is stored in `~/.scrapeless/config.json` with restricted permissions.

### Available Configuration Options

| Config Key | Description | Example |
|------------|-------------|---------|
| `apiKey` | Your Scrapeless API token (required) | `sk_xxx...` |
| `apiVersion` | API version (v1 or v2) | `v2` (default) |
| `sessionTtl` | Session timeout in seconds | `300` |
| `sessionName` | Session name for identification | `my-session` |
| `sessionRecording` | Enable session recording | `true` or `false` |
| `proxyUrl` | Custom proxy URL | `http://user:pass@proxy.com:8080` |
| `proxyCountry` | Proxy country code | `US`, `UK`, `AU` |
| `proxyState` | Proxy state/province | `CA`, `NSW` |
| `proxyCity` | Proxy city | `Los Angeles`, `sydney` |
| `fingerprint` | Browser fingerprint | `chrome`, `firefox` |
| `userAgent` | Custom user agent string | `Mozilla/5.0...` |
| `platform` | Platform type | `Windows`, `Linux`, `macOS`, `iOS`, `Android` |
| `screenWidth` | Screen width in pixels | `1920`, `2560` |
| `screenHeight` | Screen height in pixels | `1080`, `1440` |
| `timezone` | Timezone | `America/New_York`, `Asia/Shanghai` |
| `languages` | Comma-separated language codes | `en,zh-CN` |
| `debug` | Enable debug logging | `true` or `false` |

**Configuration Priority**: Config file > Environment variable (only for `apiKey`)

## Common Patterns for AI Agents

### Form Submission

```bash
# Create a session first
SESSION_ID=$(scrapeless-scraping-browser new-session --name "form-test" --ttl 600 --json | jq -r '.taskId')

# Use the session for all operations
scrapeless-scraping-browser --session-id $SESSION_ID open https://example.com/signup
scrapeless-scraping-browser --session-id $SESSION_ID snapshot -i
scrapeless-scraping-browser --session-id $SESSION_ID fill @e1 "Jane Doe"
scrapeless-scraping-browser --session-id $SESSION_ID fill @e2 "jane@example.com"
scrapeless-scraping-browser --session-id $SESSION_ID click @e3
scrapeless-scraping-browser --session-id $SESSION_ID wait --load networkidle
```

### Data Extraction

```bash
# Create a session
SESSION_ID=$(scrapeless-scraping-browser new-session --name "scraping-test" --ttl 600 --json | jq -r '.taskId')

# Extract data
scrapeless-scraping-browser --session-id $SESSION_ID open https://example.com/products
scrapeless-scraping-browser --session-id $SESSION_ID snapshot -i --json
scrapeless-scraping-browser --session-id $SESSION_ID get text @e5 --json
```

### Session Persistence

```bash
# Create a long-lived session for login
SESSION_ID=$(scrapeless-scraping-browser new-session --name "persistent-session" --ttl 3600 --json | jq -r '.taskId')

# Login
scrapeless-scraping-browser --session-id $SESSION_ID open https://app.example.com/login
scrapeless-scraping-browser --session-id $SESSION_ID snapshot -i
scrapeless-scraping-browser --session-id $SESSION_ID fill @e1 "username"
scrapeless-scraping-browser --session-id $SESSION_ID fill @e2 "password"
scrapeless-scraping-browser --session-id $SESSION_ID click @e3

# Later, reuse the same session (cookies and state are preserved)
scrapeless-scraping-browser --session-id $SESSION_ID open https://app.example.com/dashboard
scrapeless-scraping-browser --session-id $SESSION_ID snapshot -i
```

## Snapshot Options

The `snapshot` command supports filtering to reduce output size:

```bash
scrapeless-scraping-browser snapshot                    # Full accessibility tree
scrapeless-scraping-browser snapshot -i                 # Interactive elements only
scrapeless-scraping-browser snapshot -i -C              # Include cursor-interactive elements
scrapeless-scraping-browser snapshot -c                 # Compact mode
scrapeless-scraping-browser snapshot -d 3               # Limit depth to 3 levels
scrapeless-scraping-browser snapshot -s "#main"         # Scope to CSS selector
```

| Option | Description |
|--------|-------------|
| `-i, --interactive` | Only show interactive elements |
| `-C, --cursor` | Include cursor-interactive elements |
| `-c, --compact` | Remove empty structural elements |
| `-d, --depth <n>` | Limit tree depth |
| `-s, --selector <sel>` | Scope to CSS selector |

## Selectors

### Refs (Recommended for AI)

```bash
# Get snapshot with refs
scrapeless-scraping-browser snapshot -i
# Output: @e1 [button] "Submit", @e2 [input] "Email"

# Use refs to interact
scrapeless-scraping-browser click @e1
scrapeless-scraping-browser fill @e2 "test@example.com"
```

### CSS Selectors

```bash
scrapeless-scraping-browser click "#submit-button"
scrapeless-scraping-browser fill ".email-input" "test@example.com"
```

### Text Selectors

```bash
scrapeless-scraping-browser click "text=Submit"
```

## Live Preview

Get a live preview URL to watch your browser session in real-time:

```bash
# Get live preview URL
scrapeless-scraping-browser live

# Or for specific session
scrapeless-scraping-browser live <taskId>
```

## Agent Mode (JSON Output)

Use `--json` for machine-readable output:

```bash
scrapeless-scraping-browser snapshot -i --json
scrapeless-scraping-browser get text @e1 --json
scrapeless-scraping-browser is visible @e2 --json
```

## CLI Options

| Option | Description |
|--------|-------------|
| `--session-id <id>` | Connect to specific Scrapeless session (recommended for all operations) |
| `--json` | Output in JSON format |
| `--debug` | Enable debug logging |

**Important**: The `--session-id` parameter is supported by all browser operation commands (open, click, fill, snapshot, screenshot, etc.). Always use it to ensure consistent session usage.

## Error Handling

The CLI provides clear error messages for common issues:

- **Authentication errors**: Check your API key is set correctly via `config set apiKey YOUR_KEY` or `export SCRAPELESS_API_KEY=YOUR_KEY`
- **Session not found**: The session may have expired or been closed
- **Network errors**: Check your internet connection
- **Timeout errors**: Increase session timeout via `config set sessionTtl 600`

## Debugging

Enable debug mode to see detailed logs:

```bash
scrapeless-scraping-browser config set debug true
scrapeless-scraping-browser open example.com
```

Or use the `--debug` flag:

```bash
scrapeless-scraping-browser --debug open example.com
```

## API Versions

The CLI supports both v1 and v2 of the Scrapeless Gateway API. Set via config:

```bash
# Use v2 API (default)
scrapeless-scraping-browser open example.com

# Use v1 API
scrapeless-scraping-browser config set apiVersion v1
scrapeless-scraping-browser open example.com
```

## How It Works

1. **Session Creation**: Use `new-session` to create a new Scrapeless browser session with custom parameters
2. **Session Management**: Connect to specific sessions using `--session-id` or let the CLI manage sessions automatically
3. **Cloud Execution**: All browser operations run on Scrapeless infrastructure with residential IPs and anti-detection
4. **Element References**: Use AI-friendly @ref system for reliable element interaction

## API Client (TypeScript/Node.js)

For programmatic access, use the TypeScript API client:

```typescript
import { ScrapelessAPI } from 'scrapeless-scraping-browser';

const api = new ScrapelessAPI({
  apiKey: process.env.SCRAPELESS_API_KEY
});

// Create a browser session
const session = await api.browser.createSession({
  sessionName: 'my-automation',
  sessionTTL: 1800,
  proxyCountry: 'US'
});

// Get live preview URL
const liveUrl = await api.browser.getLiveUrl(session.taskId);

// Check your credits
const credits = await api.user.getCredits();
console.log(`Remaining credits: ${credits}`);
```

See [API Client Architecture](./docs/api/api-client-architecture.md) for complete documentation.

## Limitations

- Runs exclusively on Scrapeless cloud infrastructure (no local browser support)
- Sessions automatically terminate when connection is closed
- CLI `--session-id` parameter has architectural limitations for reconnection

## Updates

Check for updates and install the latest version:

```bash
# Check current version
scrapeless-scraping-browser --version

# Update via npm
npm update -g scrapeless-scraping-browser
```

## Acknowledgments

Built on top of [agent-browser](https://github.com/vercel-labs/agent-browser) by Vercel Labs, adapted for Scrapeless Cloud Browser.

## 🎁 Start for Free
We believe in our tech. That's why every new account comes with **Free Trial Credits**. 
*   **No Credit Card Required** for initial testing.
*   **Up to 100+ free hours** to benchmark our bypass success rates.

[Join Scrapeless community to **Claim Your Free Credits Now →**](https://discord.gg/stFPK2xKHY)

## 🔗 Related resources
*   **[Scrapeless Scraping Browser Guide](https://docs.scrapeless.com/en/scraping-browser/quickstart/introduction/)**
*   **[Scrapeless LLM Scraper](https://docs.scrapeless.com/en/llm-chat-scraper/quickstart/introduction/)**
*   **[Scrapeless Universal Scraping API](https://docs.scrapeless.com/en/universal-scraping-api/quickstart/introduction/)**
*   **[Scrapeless WebUnlocker](https://github.com/scrapeless-ai/webunlocker-skill)**
*   **[GitHub Issues](https://github.com/scrapeless-ai/scraping-browser-skill/issues)**

##  Contact Us

For questions, suggestions, or collaboration inquiries, feel free to contact us via:

*   **Official Website:** [Scrapeless.com](https://www.scrapeless.com)
*   **Email:** market@scrapeless.com
*   **Community:** [Join our Discord](https://discord.gg/scrapeless)
*   **Community:** [Join our Telegram](https://t.me/+uELlyZh2JGw1M2Ux)


## License

Apache-2.0

## Contributing

Contributions are welcome! Please open an issue or submit a pull request.
