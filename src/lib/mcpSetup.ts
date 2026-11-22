/**
 * The prompt Settings → AI offers for connecting an AI assistant to Jotter's MCP server. The
 * user pastes it into the assistant, which then adds the server to its own configuration.
 *
 * The prompt is for a model, not for people, so it stays in English whatever the UI language.
 */

/** The command an MCP client runs: the installed CLI, or `jotter` from PATH when none is bundled. */
export function mcpCommand(cliPath: string | null): string {
  return cliPath ?? 'jotter';
}

/** Arguments for the MCP server. `--read-only` hides and refuses every tool that changes notes. */
export function mcpArgs(readOnly: boolean): string[] {
  return readOnly ? ['--read-only', 'mcp'] : ['mcp'];
}

/** Keeps paths literal in the named shell, including backslashes and shell metacharacters. */
function shellQuote(value: string, shell: 'powershell' | 'posix'): string {
  if (/^[a-zA-Z0-9_./:-]+$/.test(value)) return value;
  return shell === 'powershell'
    ? `'${value.replaceAll("'", "''")}'`
    : `'${value.replaceAll("'", "'\"'\"'")}'`;
}

export function mcpSetupPrompt(cliPath: string | null, readOnly: boolean): string {
  const command = mcpCommand(cliPath);
  const args = mcpArgs(readOnly);
  const shellLine = (shell: 'powershell' | 'posix') => [shellQuote(command, shell), ...args].join(' ');
  const jsonEntry = JSON.stringify({ jotter: { command, args } }, null, 2);
  const access = readOnly
    ? 'It is read-only: you can search and read notes, but not change them.'
    : 'You can search, read, create, edit, append to, tag, rename, and delete notes, and add to my daily note. Changed notes keep their previous text in version history, and deleted notes go to the system trash.';
  return `Set up the Jotter MCP server for me.

Jotter is my Markdown notes app. Its command-line tool runs an MCP server over stdio that gives you tools for my notes, plus a shared context note that records what I am working on. ${access} With no --vault argument, it opens the vault I last opened in the Jotter app.

Server:
- Name: jotter
- Transport: stdio
- Command: ${command}
- Arguments: ${JSON.stringify(args)}

Steps:
1. Work out which MCP client you are running in, and add the server at user level so it works in every project. Keep every server that is already configured.
   Use the command for your shell; keep its quoting exactly as shown.
   - Claude Code (PowerShell): claude mcp add --scope user jotter -- ${shellLine('powershell')}
   - Claude Code (Bash/Zsh): claude mcp add --scope user jotter -- ${shellLine('posix')}
   - Codex CLI (PowerShell): codex mcp add jotter -- ${shellLine('powershell')}
   - Codex CLI (Bash/Zsh): codex mcp add jotter -- ${shellLine('posix')}
   - Claude Desktop, Cursor, Windsurf, VS Code, or another client with a JSON config: add this under "mcpServers" (VS Code uses "servers"):
${jsonEntry}
   If you cannot change your own configuration, tell me the file to edit and the exact entry to add.
2. If the client must restart to load the server, tell me.
3. Once the jotter tools are available, call get_context and summarize what it says.

From then on, call get_context at the start of each conversation${
    readOnly
      ? '.'
      : `, and call update_context when my focus, decisions, or open questions change. When a conversation reaches a decision, a summary, or open questions worth keeping, save them with save_conversation under a short title, and reuse that title for later saves in the same conversation.`
  } If I ask to work in another vault, use list_vaults and open_vault.`;
}
