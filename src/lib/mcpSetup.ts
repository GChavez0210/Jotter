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
  const access = `You see only the notes I select in Jotter under Settings > AI, plus the AI folder, where my context and instructions for you live. ${
    readOnly
      ? 'It is read-only: you can search and read those notes, but not change them.'
      : 'You can create, edit, tag, rename, and delete notes only in the AI folder; everything else is read-only. Changed notes keep their previous text in version history, and deleted notes go to the system trash.'
  }`;
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
3. Once the jotter tools are available, call get_context. If setUp is false, ask me to set up the AI folder in Jotter under Settings > AI, and wait. Otherwise follow the instructions it returns, read AI/Preferences.md, and summarize what you learned.${
    readOnly
      ? ''
      : `
4. Interview me: ask a few short questions about my work, my current focus, and how I like answers. Then save what I told you about my work and focus with update_context. Write my answer preferences to AI/Preferences.md. If Jotter refuses that edit, show me the text so I can paste it in myself. Keep what those notes already say unless I change it.`
  }

From then on, call get_context at the start of each conversation${
    readOnly
      ? '.'
      : `, and call update_context when my focus, decisions, or open questions change. Log the work you finish with append_to_daily. When a conversation reaches a decision, a summary, or open questions worth keeping, save them with save_conversation under a short title, and reuse that title for later saves in the same conversation.`
  } If I ask to work in another vault, use list_vaults and open_vault.`;
}
