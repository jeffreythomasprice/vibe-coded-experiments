import { Events, type Client, type Message } from "discord.js";
import { logger } from "../logger";

function resolveMentions(message: Message): string {
  return message.content.replace(/<@!?(\d+)>/g, (match, id) => {
    const user = message.mentions.users.get(id);
    if (!user) return match;
    const tag = user.bot ? "bot" : "user";
    const member = message.mentions.members?.get(id);
    const displayName = member?.displayName;
    const name = displayName && displayName !== user.username ? `${displayName} (${user.username})` : user.username;
    return `@${name}[${tag}]`;
  });
}

export const name = Events.MessageCreate;

export function execute(message: Message) {
  if (message.author.bot) return;

  const nickname = message.member?.displayName;
  const author = nickname && nickname !== message.author.username
    ? `${nickname} (${message.author.tag})`
    : message.author.tag;
  const content = resolveMentions(message) || "[no content]";

  if (message.channel.isDMBased()) {
    logger.info({ type: "DM", author, content }, "Discord message");
    return;
  }

  const channel = "name" in message.channel ? `#${message.channel.name}` : `#${message.channelId}`;
  const guild = message.guild?.name ?? "unknown";

  if (message.client.user && message.mentions.has(message.client.user)) {
    logger.info({ type: "MENTION", channel, guild, author, content }, "Discord message");
    return;
  }

  logger.info({ type: "CHANNEL", channel, guild, author, content }, "Discord message");
}
