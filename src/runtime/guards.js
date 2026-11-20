import { normalizeWhitespace } from '../shared/translator.js';

export const SKIP =
  'script,style,pre,code,textarea,input,[contenteditable],iframe,webview,' +
  '[data-no-translate],[translate="no"],[data-antigravity-zh-control],[data-agy-zh-control],[data-agy-zh-menu],.monaco-editor,' +
  '[data-sidebar-row-id],[data-tooltip-id$="-name"],[data-testid^="automation-card-"],' +
  '[data-testid="project-selector-item"],[data-model-base],[data-harness],[data-mention-menu],' +
  '[data-testid="ask-question-option-hover-card"],[data-testid="sidecar-iframe"],[data-testid="sidecar-iframe-slot"],' +
  '[data-testid="automation-name"],[data-testid="sidecar-project-link"],[data-testid="automation-instructions-markdown"],' +
  '[data-testid="automation-instructions-text"],[data-testid="automation-run"],[data-testid="sidecar-model-option"],' +
  '[data-testid="new-sidecar-project"],[data-testid="new-sidecar-model"],[data-testid="diff-view-mode-detail"],' +
  '[data-testid="diff-view-mode-option"] + span,[data-testid="terminal-tab-surface"],[data-testid="browser-url-display"],' +
  '[data-testid="voice-picker-name"],[data-testid="customization-card"] a,[data-testid="customization-card"] .font-medium > button,' +
  '[data-testid="featured-carousel-headline"],[data-testid^="bundle-card-"] h3,' +
  '[data-testid="builtin-browser-download-dir-value"],[data-testid="gcp-project-combobox-list"],' +
  '[data-testid="webview-device-chooser-option"],[data-testid="webview-prompt-message"],' +
  '[data-testid="webview-download-filename"],[data-testid="webview-download-open"],' +
  '[data-testid="url-security-host"],[data-testid="url-security-origin"],' +
  '[data-testid="lsp-symbol-hover-card"],[data-testid="lsp-reference-item"],[data-testid="csv-scroll-viewport"],' +
  '[data-testid="coordinator-worker-card"],[data-testid="coordinator-chat-header"],' +
  '[data-testid="permission-download-details"] dd,[data-testid="resource-link"],' +
  '[data-testid="ssh-host-project-item"],[data-testid="project-selector-github-repo-item"],' +
  '[data-testid^="github-repo-option-"],[data-testid="ssh-connect-host-option"] > span:first-of-type,' +
  '[data-testid="ssh-connect-error"] span.font-mono,[data-testid="ssh-machine-switcher-wsl-distro"] .truncate,' +
  '[data-testid="plugins-visibility-option"],[data-testid="plugin-sidebar-item"],[data-testid="plugin-sidebar-mcp-app-item"],' +
  '[data-testid="branch-repo-label"],[data-testid="branch-option"],' +
  '[data-agy-chip],[data-agy-pill],[data-agy-window-label],[data-agy-filter-item]:not(:first-child),' +
  '[data-agy-header] > span.truncate,[data-agy-strip-row] > span.truncate,' +
  '.bg-transparent.min-w-0 > span.truncate[data-tooltip-id],' +
  '[data-testid="browser-site-data-site"] > div > span:first-of-type,[data-testid="external-link-confirm-uri"],' +
  '[data-testid="jj-previous-worktree-option"],[data-testid="plugin-source-caution-modal"] span.font-medium.truncate,' +
  '[data-testid="bundle-detail"] h1,[data-testid="step-plan-item"],' +
  '[data-testid="lsp-hover-signature"],[data-testid="lsp-hover-body"],[data-testid="lsp-hover-tags"]';

// Model/user data nodes, even when rendered inside a UI popup.
export const CONTENT =
  '[data-testid="ask-question-header-text"],[data-testid="ask-question-option-text"],' +
  '[data-testid="planner-response-text"],[data-testid="progress-update-response-text"],[data-bubble-side="agent"],' +
  '[data-testid="user-message-content"],[data-testid="side-question-followup-user"],[data-testid="voice-transcript-summary"],' +
  '[data-testid="markdown-code-block-preview"],[data-testid="background-task-command"],' +
  '[data-testid="setup-script-output"],[data-testid="browser-subagent-thinking"],[data-testid="comment-card-text"]';

export const POPUPS =
  '.z-\\[6000\\],[role="menu"],[data-select-popup],[data-popover-popup],' +
  '[data-hover-card-popup],[role="tooltip"]';

export const UI_TARGETS =
  POPUPS + ',[aria-label="Sidebar"],[aria-label="Settings"],[role="dialog"],h1,' +
  '[data-testid="project-selector-trigger"],[data-testid="agent-input-box"],p.pointer-events-none,' +
  '.files-changed-header,[data-testid="cleared-step-notice"],.flex-wrap,.proceed-button,' +
  'button > span.whitespace-nowrap,button[aria-label],span.font-medium.text-sm,' +
  '[data-testid="interaction-skip-button"],[data-testid="interaction-continue-button"],' +
  '[data-testid="permission-details-toggle"],[data-testid="permission-target-editor"],' +
  '[data-testid="permission-download-details"],[data-testid="ask-question-writein"],' +
  '[data-testid="ask-question-progress"],[data-testid="thinking-collapsible-trigger"],' +
  '[data-testid="artifact-card-split-button"],[data-testid="open-in-new-window-action"],' +
  '[data-testid="overview-panel"],[data-testid="cloud-triggers-view"],' +
  '[data-testid="trigger-InlineCreator"],[data-testid="automation-detail-view"],' +
  '[data-testid="new-sidecar-modal"],[data-testid="sidecars-view"],' +
  '[data-testid="automation-detail-page"],[data-testid="sidecar-detail"],' +
  '[data-testid="bottom-terminal-panel"],[data-testid="bottom-terminal-panel-title"],' +
  '[data-testid="bottom-terminal-panel-starting"],[data-testid="bottom-terminal-panel-empty"],' +
  '[data-testid="terminal-tab-connecting"],[data-testid="terminal-password-warning"],' +
  '[data-testid="diff-view-mode-trigger"],[data-testid="diff-view-mode-option"],' +
  '[data-testid="builtin-browser-reload"],[data-testid="browser-url-input"],' +
  '[data-testid="webview-download-show-in-folder"],[data-testid="webview-download-cancel"],' +
  '[data-testid="webview-download-remove"],[data-testid="webview-downloads-close"],' +
  '[data-testid="conversation-telemetry-icon-button"],[data-testid="conversation-telemetry-indicator"],' +
  '[data-testid="voice-picker"],[data-testid="customizations-search"],' +
  '[data-testid="customizations-marketplace"],[data-testid="customizations-installed"],[data-testid="plugins-tabs"],' +
  '[data-testid="selection-quote-button"],[data-testid="selection-btw-button"],' +
  '[data-testid="selection-comment-button"],[data-testid="selection-popup-quote-button"],[data-testid="selection-code-search-button"],' +
  '[data-testid="comment-box"],[data-testid="comment-card"],' +
  '[data-testid="side-question-panel"],[data-testid="confirm-delete-side-question"],' +
  '[data-testid="create-coordinator-submit"],[data-testid="coordinator-settings-save"],' +
  '[data-testid="new-coordinator-button"],[data-testid="coordinator-section-header"],' +
  '[data-testid="file-tree-filter-input"],[data-testid="file-tree-filter-empty"],[data-testid="file-tree-vcs-dot"],' +
  '[data-testid="csv-file-viewer"],[data-testid="lsp-peek-references-close"],' +
  '[data-testid="url-security-indicator"],[data-testid="browser-cert-interstitial"],' +
  '[data-testid="builtin-browser-caa-open-external"],[data-testid="onboarding-auth-code-panel"],' +
  '[data-testid="automation-model-unavailable"],' +
  '[data-testid="computer-permissions-card"],[data-testid="ssh-connect-dialog"],' +
  '[data-testid="ssh-machine-switcher"],[data-testid="ssh-host-label"],' +
  '[data-testid="permissions-selector-trigger"],[data-testid="provider-resource-filter"],' +
  '[data-testid="google-drive-connect-view"],[data-testid="setup-script-save-button"],' +
  '[data-testid="setup-script-validate-button"],[data-testid="setup-script-status"],' +
  '[data-testid="setup-script-copy-button"],[data-testid="plugins-section-header"],' +
  '[data-testid="plugins-visibility-trigger"],[data-testid="webview-url-bar-draft-capsule"],[data-testid="webview-url-bar-downloads"],' +
  '[data-testid="side-panel-new-tab-button"],[data-testid="side-panel-pending-terminal-tab"],[data-testid="agent-browser-cross-window-banner"],' +
  '[data-testid="file-tree-filter-error"],[data-testid="model-selector-refresh"],' +
  '[data-testid="onboarding-switch-to-auth-code"],[data-testid="onboarding-brand-shell"],[data-agy-agents-pane],' +
  '[data-testid="project-folder-selection-step"],[data-testid="cloud-resource-disclosure-modal"],' +
  '[data-testid="title-menu-bar"],[data-testid="title-menu-bar-item"],[data-testid="title-menu-bar-option"],' +
  '[data-testid="browsing-data-row"],[data-testid="browser-site-data-dialog"],[data-testid="browser-clear-data-confirm"],' +
  '[data-testid="browser-loopback-loading-state"],[data-testid="external-link-confirm-modal"],' +
  '[data-testid="plugin-source-caution-modal"],[data-testid="command-hook-warning"],' +
  '[data-testid="jj-worktree-selector-trigger"],[data-testid="jj-worktree-menu"],[data-testid="jj-previous-worktrees-menu"],' +
  '[data-testid="bundles-page"],[data-testid="bundle-detail"],[data-testid="marketplace-bundles-banner"],' +
  '[data-testid="side-question-insert-to-main"],[data-testid="jetbox-access-blocked-screen"],' +
  '[data-testid="ssh-sign-in-subtitle"],[data-testid="toolbar-text-selection-button"],' +
  '[data-testid="toolbar-fit-mode-button"],[data-testid="toolbar-overflow-trigger"],' +
  '[data-testid="post-onboarding-terms-body"],[data-testid="post-onboarding-welcome-modal"],' +
  '[data-testid="home-greeting"],' +
  'input[placeholder="Search conversations..."]';

export const settingsNavIds = new Set([
  'General', 'App', 'Appearance', 'Browser', 'Project 4K',
  'Interface Layout', 'Shortcuts', 'Notifications', 'Models', 'Customizations', 'Developer',
  'Running', 'Provide Feedback', 'Give Feedback', 'Account', 'Conversations',
  'Projects', 'Environments',
]);

export const supportedSettingsHeadings = [
  'General', 'Application', 'Appearance', 'Browser',
  'Browser Actuation Permissions', 'Models & Usage', 'Shortcuts', 'Account',
  'Provide Feedback', 'Give Feedback', 'Customizations', 'Notifications',
  'Project 4K', 'Interface Layout', 'Permissions', 'Global Permissions',
  'GitHub Permissions', 'File Permissions', 'Network Permissions',
  'Terminal Commands', 'MCP Tools', 'Add MCP Servers', 'Manage Hooks',
  'Build With Google Plugins', 'Select your license', 'Developer',
  'Workspaces', 'Projects', 'Not in Project', 'Environments', 'Running (Dev only)',
];

export const fallbackSettingsTexts = new Set([
  'Settings', 'General', 'Application', 'Appearance', 'Browser',
  'Browser Actuation Permissions', 'Models', 'Conversations', 'Shortcuts',
  'Provide Feedback', 'Give Feedback', 'Account', 'Customizations',
  'Notifications', 'Project 4K', 'Interface Layout', 'Permissions',
  'Global Permissions', 'GitHub Permissions', 'Developer', 'Back',
  'Workspaces', 'Projects', 'Not in Project', 'Environments',
  'Settings for conversations outside of projects.',
  'Settings for conversations outside of workspaces.',
  'Agent settings and permissions for conversations outside of projects.',
  'Agent settings and permissions for conversations outside of workspaces.',
]);

export const allowedFragments = new Set([
  '(ADC)', 'Allowed Domains', 'Also includes',
  'Organization MCP Allowlist (Read-Only)', 'Powered by Gemini Live ·', 'Terms of Service',
  'This will permanently delete', 'within it.', 'including', 'Zipping', 'in the top left.',
  'menu)', 'to be installed.',
  'to be installed. The browser subagent can be invoked by typing /browser in the conversation input box.',
  'Configure allowed and denied URLs for the browser subagent (invoked with /browser). Requires',
  'For Antigravity Business consumption options, see', 'Visit',
  'to ask for content changes for legal reasons.', 'to have the agent generate a plan.',
  'when working', 'in this project', 'in these conversations', 'when working in this project.',
  'Click', 'to open macOS Privacy & Security.', 'Then create the project in its window.',
  "Couldn't connect to", 'No hosts in', 'yet. Type one above.',
  'Always open', 'links without asking',
  'Inspect live bindings or record custom key chords filtered to', 'commands.',
]);

export const navigationLabels = new Set([
  'New Conversation', 'Conversation History', 'Automations',
  'Scheduled Tasks', 'Customizations', 'Settings', 'Provide Feedback', 'Give Feedback',
  'Chief of Staff',
]);

export const sidebarTestIds = new Set([
  'automations-button', 'settings-button',
  'new-conversation-button', 'history-button', 'customizations-button', 'concierge-button',
]);

export const sidebarPlaceholders = new Set([
  'No conversations yet', 'No pinned conversations',
  'Drop to add to group', 'See all',
]);

export const projectTriggerLabels = new Set([
  'No Project', 'Select Project', 'Select Workspace',
  'New Workspace', 'No Workspace',
]);

export const chatInputPlaceholders = new Set([
  'Ask anything, @ to mention, / for actions',
  'Ask anything, @ to mention', 'Ask anything',
  'Message subagent, @ to mention, / for actions', 'Message subagent, @ to mention',
  'Listening. Ask anything.', 'Leave a comment', 'Edit comment',
]);

export const historySafeTexts = new Set([
  'Conversation History',
  'Manage and revisit your past agent sessions.', 'Active', 'Archived', 'Select', 'Done',
  'Select all', 'Deselect all', 'Archive', 'Unarchive', 'Delete', 'Show more',
  'No matching conversations.', 'No archived conversations.', 'No conversations yet.',
  'No conversations found', 'Display Options', 'This page is going away soon',
  'Search conversations with', 'in the top left.', 'View archived in the sidebar', 'menu)',
]);

export const overviewSafeTexts = new Set([
  'Overview', 'Review', 'Files', 'Terminal', 'Plugins',
  'Subagents', 'Coordinators', 'Side Questions', 'Environments', 'Background Tasks', 'Goals',
  'Documents', 'Artifacts', 'Links', 'Modified Files', 'Uploads', 'Extensions', 'No subagents',
  'No coordinators', 'No links', 'No uploads', 'No plugins available', 'Needs attention',
  'Approve', 'Deny', 'Blocked, needs input', 'Agents', 'Chief of Staff Dashboard',
  'Customize Sections', 'Show less', 'Open in Side Panel', 'Open in Built-in Browser',
  'Open in New Tab', 'Open in External Browser',
]);

export const permissionOptionSafeTexts = new Set([
  'Yes, allow', 'Yes, allow this time',
  'Yes, allow this download', 'Yes, save rule in this conversation',
  'Yes, and always allow in this conversation', 'Yes, save rule globally',
  'Yes, and always allow',
]);

export const permissionDownloadLabels = {
  File: '文件',
  From: '来源',
  'Redirected from': '重定向自',
  Type: '类型',
  'Save to': '保存至',
};

export const approvalButtonSafeTexts = new Set([
  'Allow Once', 'Always Allow', 'Deny', 'Confirm',
  'Accept', 'Reject', 'Configure', 'Try Again',
]);

export const automationSafeTexts = new Set([
  'Automations', 'Scheduled Tasks', 'Scheduled', 'Always on',
  'Paused', 'Next run', 'Last run', 'Instructions', 'Runs', 'All runs', 'No runs yet',
  'No instructions.', 'No upcoming run.', 'Schedule', 'Model', 'Events', 'Run Now', 'Edit',
  'Save', 'Saved', 'Reset', 'Cancel', 'Delete', 'See less', 'Show all', 'Show less',
  'Paused. Resume it to schedule the next run.', 'Sidecar feature is not available.',
  'Runs a command', 'Could not start.', 'Starting', 'Paused.',
  'No automations configured.', 'No scheduled tasks configured.',
  'No automations matching your search', 'No scheduled tasks matching your search',
  'Back to Automations', 'Back to Scheduled Tasks', 'Loading runs…',
  'This sidecar failed to start.',
]);

export function createGuards(state) {
  function protectedContent(element, allowPermissionOption = false) {
    for (let content = element.closest(CONTENT); content; content = content.parentElement?.closest(CONTENT)) {
      if (allowPermissionOption && content.matches('[data-testid="ask-question-option-text"]')) continue;
      if (content.matches('[data-testid="ask-question-header-text"]') &&
        element.closest('[data-testid="permission-details-toggle"]')) continue;
      return true;
    }
    return false;
  }

  function protectedNode(node, allowPermissionOption = false) {
    const element = node.nodeType === 1 ? node : node.parentElement;
    if (!element || element.closest(SKIP) || protectedContent(element, allowPermissionOption)) return true;
    if (element.closest('[data-agy-header] > button:first-of-type')) return true;
    const sshConfigName = element.closest('[data-testid="ssh-config-file-name"]');
    if (sshConfigName && state.originalText(sshConfigName) !== '~/.ssh/config (default)') return true;
    const branchPill = element.closest('[data-testid="branch-pill-label"]');
    if (branchPill && !['Select branch', 'Repo default'].includes(state.originalText(branchPill))) return true;
    const worktreeLabel = element.closest('[data-testid="jj-worktree-selector-label"]');
    if (worktreeLabel && !['Local', 'New Worktree', 'Unknown Worktree'].includes(state.originalText(worktreeLabel))) return true;
    const titleMenuItem = element.closest('[data-testid="title-menu-bar-item"]');
    if (titleMenuItem && !['Workspace', 'View', 'Window'].includes(state.originalText(titleMenuItem))) return true;
    const button = element.closest('button');
    if (!button) return false;
    if (/[^\s]+@[^\s]+/.test(button.textContent)) return true;
    const testId = button.getAttribute('data-testid');
    return !!(testId?.startsWith('settings-nav-item-') && !settingsNavIds.has(testId.slice(18)));
  }

  function skippedControl(el) {
    return el.matches('input,textarea') ? el.parentElement?.closest(SKIP) : el.closest(SKIP);
  }

  function buttonLeadLabel(btn) {
    const first = btn.firstChild;
    if (first?.nodeType === 3) return normalizeWhitespace(state.english(first));
    const trunc = btn.querySelector('span.truncate');
    return trunc ? state.originalText(trunc) : state.originalText(btn);
  }

  function isPermissionCard(card) {
    if (!card || card.closest(SKIP) || protectedContent(card)) return false;
    const header = card.querySelector('[data-testid="ask-question-header-text"]');
    const input = card.querySelector('[data-testid="ask-question-writein"]');
    if (![header, input].every((el) => el?.closest('[tabindex="-1"]') === card)) return false;
    if (card.querySelector('[data-testid="ask-question-collapse-button"]')?.closest('[tabindex="-1"]') === card) return false;
    const hasMarker = [...card.querySelectorAll(
      '[data-testid="permission-details-toggle"],[data-testid="permission-target-editor"],' +
      '[data-testid="permission-download-details"],[data-testid="ask-question-header-icon"]'
    )].some((el) => el.closest('[tabindex="-1"]') === card);
    if (!hasMarker) return false;
    const placeholder = state.placeholderRecords.get(input)?.original ?? input.getAttribute('placeholder');
    return normalizeWhitespace(placeholder || '') === '(tell the agent what to do instead)';
  }

  return {
    protectedContent,
    protectedNode,
    skippedControl,
    buttonLeadLabel,
    isPermissionCard,
  };
}
