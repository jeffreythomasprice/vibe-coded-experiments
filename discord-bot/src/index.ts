import { loadConfig } from "./config";
import { logger, initLogger } from "./logger";

const args = process.argv.slice(2);
const command = args[0];

if (command === "serve") {
  const { Client, Events, GatewayIntentBits, Partials } = await import("discord.js");
  const messageCreate = await import("./events/messageCreate");
  const { messageRequestSchema, notificationRequestSchema } = await import("./schemas");

  const token = process.env.DISCORD_TOKEN;
  if (!token) {
    logger.fatal("DISCORD_TOKEN environment variable is required");
    process.exit(1);
  }

  const config = await loadConfig();
  initLogger(config.logFile);

  const client = new Client({
    intents: [
      GatewayIntentBits.Guilds,
      GatewayIntentBits.GuildMessages,
      GatewayIntentBits.DirectMessages,
      GatewayIntentBits.MessageContent,
      GatewayIntentBits.GuildMembers,
    ],
    partials: [Partials.Channel, Partials.Message],
  });

  client.once(Events.ClientReady, (readyClient) => {
    logger.info({ tag: readyClient.user.tag }, "Discord client ready");

    async function findDefaultRecipient() {
      let user = readyClient.users.cache.find(
        (u) => u.username === config.defaultRecipient
      );

      if (!user) {
        for (const guild of readyClient.guilds.cache.values()) {
          const members = await guild.members.fetch({ query: config.defaultRecipient, limit: 1 });
          const match = members.find((m) => m.user.username === config.defaultRecipient);
          if (match) {
            user = match.user;
            break;
          }
        }
      }

      return user;
    }

    Bun.serve({
      hostname: config.host,
      port: config.port,
      async fetch(req) {
        const start = performance.now();
        const url = new URL(req.url);
        const method = req.method;
        const pathname = url.pathname;

        let response: Response;

        const requestBody = await req.clone().json().catch(() => null);

        if (req.method === "POST" && url.pathname === "/message") {
          const body = requestBody;
          const result = messageRequestSchema.safeParse(body);

          if (!result.success) {
            response = Response.json({ error: result.error.message }, { status: 400 });
          } else {
            const user = await findDefaultRecipient();

            if (!user) {
              response = Response.json({ error: "Recipient not found" }, { status: 404 });
            } else {
              try {
                await user.send(result.data.message);
                response = Response.json({ success: true });
              } catch (err) {
                response = Response.json(
                  { error: `Failed to send message: ${err}` },
                  { status: 500 }
                );
              }
            }
          }
        } else if (req.method === "POST" && url.pathname === "/claude-notification") {
          const body = requestBody;
          const result = notificationRequestSchema.safeParse(body);

          if (!result.success) {
            response = Response.json({ error: result.error.message }, { status: 400 });
          } else {
            logger.info({ notification: body }, "Claude notification received");

            const parts = [];
            if (result.data.hook_event_name) parts.push(result.data.hook_event_name);
            if (result.data.message) parts.push(result.data.message);

            const text = parts.join(", ") || "(empty notification)";

            const user = await findDefaultRecipient();

            if (!user) {
              response = Response.json({ error: "Recipient not found" }, { status: 404 });
            } else {
              try {
                await user.send(text);
                response = Response.json({ success: true });
              } catch (err) {
                response = Response.json(
                  { error: `Failed to send message: ${err}` },
                  { status: 500 }
                );
              }
            }
          }
        } else {
          response = Response.json({ error: "Not found" }, { status: 404 });
        }

        const responseBody = await response.clone().json().catch(() => null);
        const duration = (performance.now() - start).toFixed(1);
        logger.info(
          { method, pathname, status: response.status, durationMs: duration, req: requestBody, res: responseBody },
          "HTTP request"
        );
        return response;
      },
    });

    logger.info({ host: config.host, port: config.port }, "HTTP server listening");
  });

  client.on(messageCreate.name, messageCreate.execute);

  client.login(token);
} else if (command === "send") {
  const message = args[1];
  if (!message) {
    logger.error("Usage: bun run src/index.ts send \"your message\"");
    process.exit(1);
  }

  const config = await loadConfig();
  const url = `http://${config.host}:${config.port}/message`;

  try {
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ message }),
    });

    const data = await response.json();
    logger.info(data, "Response");

    if (!response.ok) {
      process.exit(1);
    }
  } catch (err) {
    logger.error(err, "Request failed");
    process.exit(1);
  }
} else if (command === "claude-notification") {
  const jsonStr = args[1] ?? await Bun.stdin.text();

  const config = await loadConfig();
  const url = `http://${config.host}:${config.port}/claude-notification`;

  try {
    const parsed = JSON.parse(jsonStr);
    const response = await fetch(url, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(parsed),
    });

    const data = await response.json();
    logger.info(data, "Response");

    if (!response.ok) {
      process.exit(1);
    }
  } catch (err) {
    logger.error(err, "Request failed");
    process.exit(1);
  }
} else {
  console.log(`Usage: bun run src/index.ts <command>

Commands:
  serve                          Start the Discord bot and HTTP server
  send <message>                 Send a message via the HTTP API
  claude-notification [json]     Send a notification (reads stdin if no arg)`);
  process.exit(command === "--help" || command === "-h" ? 0 : 1);
}
