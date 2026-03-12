# Discord Bot

Discord bot built with discord.js v14 and Bun. Logs messages to the console, exposes an HTTP API for sending messages, and includes a CLI client.

## Setup

```bash
bun install
```

Create a `.env` file with your Discord bot token:

```
DISCORD_TOKEN=your_token_here
```

## Usage

```bash
bun run dev    # start server with auto-reload
bun run start  # start server without auto-reload
```

The server starts the Discord bot and an HTTP API on the configured host and port.

## Config

The server reads `discord-bot.config.json` from the working directory:

```json
{
  "host": "127.0.0.1",
  "port": 45192,
  "defaultRecipient": "jeff0587"
}
```

| Property           | Description                          | Default       |
|--------------------|--------------------------------------|---------------|
| `host`             | HTTP server bind address             | `127.0.0.1`   |
| `port`             | HTTP server port                     | `45192`       |
| `defaultRecipient` | Discord username to send messages to | `jeff0587`    |

## CLI

Send a message to the default recipient:

```bash
bun run send "hello world"
```

## Building a Binary

Compile to a standalone executable in `dist/`:

```bash
bun build src/index.ts --compile --outfile dist/discord-bot
```

Run the binary directly:

```bash
./dist/discord-bot serve          # start the bot + HTTP server
./dist/discord-bot send "hello"   # send a message
```

The binary is self-contained and doesn't require Bun to be installed at runtime. You still need a `.env` file (or `DISCORD_TOKEN` set in the environment) and `discord-bot.config.json` in the working directory.

## Running as a Service

The bot can run as a systemd user service, surviving logouts and reboots.

**One-time setup** (requires sudo):

```bash
sudo loginctl enable-linger jeff
```

Install the service file and enable it:

```bash
mkdir -p ~/.config/systemd/user
cp discord-bot.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable discord-bot.service
systemctl --user start discord-bot.service
```

**Management:**

| Action | Command |
|--------|---------|
| Start | `systemctl --user start discord-bot` |
| Stop | `systemctl --user stop discord-bot` |
| Restart | `systemctl --user restart discord-bot` |
| Status | `systemctl --user status discord-bot` |
| Logs (live) | `journalctl --user -u discord-bot -f` |
| After rebuild | `systemctl --user restart discord-bot` |

## Example Claude Code Config

{
  "hooks": {
    "Notification": [
      {
        "matcher": "",
        "hooks": [
          {
            "type": "command",
            "command": "bash -c 'INPUT=$(cat); TITLE=$(echo \"$INPUT\" | jq -r .title); MSG=$(echo \"$INPUT\" | jq -r .message); TYPE=$(echo \"$INPUT\" | jq -r .notification_type); /home/jeff/workspaces/personal/vibe-coded-experiments/discord-bot/dist/discord-bot send \"[$TYPE] $TITLE\" \"$MSG\"'"
          }
        ]
      }
    ]
  },
  ...
}