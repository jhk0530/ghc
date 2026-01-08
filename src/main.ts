import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { open } from "@tauri-apps/plugin-dialog";
import { openUrl } from "@tauri-apps/plugin-opener";
import DOMPurify from "dompurify";
import { marked } from "marked";

window.addEventListener("DOMContentLoaded", () => {
  const formEl = document.querySelector<HTMLFormElement>("#message-form");
  const inputEl = document.querySelector<HTMLInputElement>("#message-input");
  const modelSelect =
    document.querySelector<HTMLSelectElement>("#model-select");
  const fileButton =
    document.querySelector<HTMLButtonElement>("#file-picker");
  const fileContextEl =
    document.querySelector<HTMLElement>("#file-context");
  const sendButton =
    document.querySelector<HTMLButtonElement>("#send-button");
  const billingButton =
    document.querySelector<HTMLButtonElement>("#billing-link");
  const authButton = document.querySelector<HTMLButtonElement>("#github-auth");
  const authStatusEl =
    document.querySelector<HTMLElement>("#auth-status") ??
    (() => {
      const created = document.createElement("p");
      created.id = "auth-status";
      created.setAttribute("aria-live", "polite");
      document.querySelector("main")?.appendChild(created);
      return created;
    })();
  const versionEl =
    document.querySelector<HTMLElement>("#copilot-version") ??
    (() => {
      const created = document.createElement("span");
      created.id = "copilot-version";
      document.querySelector("footer")?.appendChild(created);
      return created;
    })();
  const installCopilotButton =
    document.querySelector<HTMLButtonElement>("#install-copilot");
  const reloadAppButton =
    document.querySelector<HTMLButtonElement>("#reload-app");
  const copyWrap =
    document.querySelector<HTMLElement>(".copy-wrap") ??
    (() => {
      const created = document.createElement("div");
      created.classList.add("copy-wrap", "is-hidden");
      document.querySelector(".output-wrap")?.prepend(created);
      return created;
    })();
  const copyButton =
    document.querySelector<HTMLButtonElement>("#copy-output") ??
    (() => {
      const created = document.createElement("button");
      created.id = "copy-output";
      created.classList.add("icon-button", "copy-button");
      document.querySelector(".output-wrap")?.prepend(created);
      return created;
    })();
  const historyToggle =
    document.querySelector<HTMLButtonElement>("#toggle-history") ??
    (() => {
      const created = document.createElement("button");
      created.id = "toggle-history";
      created.classList.add("icon-button", "history-toggle");
      document.querySelector(".output-wrap")?.prepend(created);
      return created;
    })();
  let statusTimer: number | undefined;
  const outputEl =
    document.querySelector<HTMLElement>("#output") ??
    (() => {
      const created = document.createElement("section");
      created.id = "output";
      created.classList.add("output");
      created.setAttribute("aria-live", "polite");
      document.querySelector("main")?.appendChild(created);
      return created;
    })();
  const historyEl =
    document.querySelector<HTMLElement>("#history") ??
    (() => {
      const created = document.createElement("section");
      created.id = "history";
      created.classList.add("history");
      created.setAttribute("aria-live", "polite");
      document.querySelector(".output-wrap")?.appendChild(created);
      return created;
    })();
  const authLabel = document.querySelector<HTMLElement>("#auth-label");
  let hasToken = false;
  let lastOutput = "";
  let copyFeedbackTimer: number | undefined;
  let isRunning = false;
  let contextPath: string | null = null;
  let contextName: string | null = null;
  const setPromptEnabled = (enabled: boolean) => {
    inputEl && (inputEl.disabled = !enabled);
    sendButton && (sendButton.disabled = !enabled);
    modelSelect && (modelSelect.disabled = !enabled);
    fileButton && (fileButton.disabled = !enabled);
  };
  const setCopyVisible = (visible: boolean) => {
    copyWrap.hidden = !visible;
    copyWrap.classList.toggle("is-hidden", !visible);
    copyWrap.classList.toggle("is-visible", visible);
  };
  setCopyVisible(false);
  const setReloadVisible = (visible: boolean) => {
    if (!reloadAppButton) return;
    reloadAppButton.hidden = !visible;
    reloadAppButton.classList.toggle("is-hidden", !visible);
  };
  setReloadVisible(false);
  const historyTooltip = historyToggle
    ?.closest(".icon-wrap")
    ?.querySelector<HTMLElement>(".icon-tooltip");
  const setHistoryVisible = (visible: boolean) => {
    historyEl.hidden = !visible;
    historyEl.classList.toggle("is-hidden", !visible);
    historyToggle.setAttribute(
      "aria-label",
      visible ? "Hide history" : "Show history",
    );
    historyToggle.setAttribute(
      "title",
      visible ? "Hide history" : "Show history",
    );
    if (historyTooltip) {
      historyTooltip.textContent = visible ? "Hide" : "History";
    }
  };
  setHistoryVisible(false);

  const appendHistory = async (promptText: string, outputText: string) => {
    const item = document.createElement("article");
    item.classList.add("history-item");
    const promptEl = document.createElement("p");
    promptEl.classList.add("history-prompt");
    promptEl.textContent = promptText;
    const outputBlock = document.createElement("div");
    outputBlock.classList.add("history-output");
    outputBlock.innerHTML = DOMPurify.sanitize(
      await marked.parse(outputText),
    );
    item.append(promptEl, outputBlock);
    historyEl.appendChild(item);
  };

  const updateTokenStatus = async () => {
    try {
      const status = await invoke<{ has_token: boolean; tail?: string }>(
        "get_token_status",
      );
      hasToken = status.has_token;
      if (status.has_token) {
        authButton?.classList.remove("is-off");
        authButton?.classList.add("is-on");
        authButton?.classList.remove("needs-login");
        authButton?.setAttribute("aria-label", "Logout");
        authButton?.setAttribute("title", "Logout");
        if (authLabel) authLabel.textContent = "Logout";
        setPromptEnabled(true);
      } else {
        authButton?.classList.remove("is-on");
        authButton?.classList.add("is-off");
        authButton?.classList.add("needs-login");
        authButton?.setAttribute("aria-label", "Login with GitHub");
        authButton?.setAttribute("title", "Login with GitHub");
        if (authLabel) authLabel.textContent = "Login";
        setPromptEnabled(false);
      }
    } catch {
      hasToken = false;
      authButton?.classList.remove("is-on");
      authButton?.classList.add("is-off");
        authButton?.classList.add("needs-login");
      authButton?.setAttribute("aria-label", "Login with GitHub");
      authButton?.setAttribute("title", "Login with GitHub");
      if (authLabel) authLabel.textContent = "Login";
      setPromptEnabled(false);
    }
  };

  void listen<{ status: "ok" | "error"; message: string }>(
    "github-login-complete",
    (event) => {
      const { status, message } = event.payload;
      authStatusEl.textContent =
        status === "ok" ? message : `Login failed: ${message}`;
      if (status === "ok") {
        void updateTokenStatus();
        if (statusTimer) window.clearTimeout(statusTimer);
        statusTimer = window.setTimeout(() => {
          authStatusEl.textContent = "";
        }, 10_000);
      }
    },
  );

  formEl?.addEventListener("submit", async (event) => {
    event.preventDefault();
    if (!inputEl) return;
    if (isRunning) return;

    const prompt = inputEl.value.trim();
    if (!prompt) return;
    const model = modelSelect?.value ?? "claude-sonnet-4.5";
    if (contextPath && !contextName) {
      contextName = contextPath.split(/[\\/]/).pop() ?? contextPath;
    }
    const contextLabel = contextName ? `./${contextName}` : null;
    const promptForHistory = contextLabel
      ? `${prompt} ${contextLabel}`
      : prompt;

    isRunning = true;
    if (sendButton) sendButton.classList.add("is-loading");
    inputEl.disabled = true;
    if (modelSelect) modelSelect.disabled = true;
    if (sendButton) sendButton.disabled = true;
    outputEl.textContent = "Running copilot...";
    setCopyVisible(false);
    copyButton.classList.remove("is-copied");
    try {
      const result = await invoke<{
        output: string;
        temp_path?: string | null;
        context_path?: string | null;
      }>("run_copilot", {
        args: {
          prompt,
          model,
          contextPath: contextPath ?? undefined,
        },
      });
      lastOutput = result.output ?? "";
      const rendered = result
        ? DOMPurify.sanitize(await marked.parse(result.output))
        : "(no output)";
      if (result) {
        outputEl.innerHTML = rendered;
        if (lastOutput.trim()) {
          setCopyVisible(true);
          await appendHistory(promptForHistory, lastOutput);
        } else {
          setCopyVisible(false);
        }
      } else {
        outputEl.textContent = rendered;
        setCopyVisible(false);
      }
    } catch (error) {
      lastOutput = "";
      setCopyVisible(false);
      outputEl.textContent =
        error instanceof Error
          ? error.message
          : String(error ?? "Unknown error");
    } finally {
      isRunning = false;
      inputEl.disabled = false;
      if (modelSelect) modelSelect.disabled = false;
      if (sendButton) {
        sendButton.disabled = false;
        sendButton.classList.remove("is-loading");
      }
      contextPath = null;
      contextName = null;
      if (fileContextEl) {
        fileContextEl.textContent = "";
        fileContextEl.title = "";
        fileContextEl.classList.add("is-hidden");
      }
    }
  });

  authButton?.addEventListener("click", async () => {
    if (statusTimer) window.clearTimeout(statusTimer);
    if (hasToken) {
      authStatusEl.textContent = "Logging out...";
      try {
        await invoke("clear_github_token");
        await updateTokenStatus();
        authStatusEl.textContent = "Logged out.";
        statusTimer = window.setTimeout(() => {
          authStatusEl.textContent = "";
        }, 10_000);
      } catch (error) {
        authStatusEl.textContent =
          error instanceof Error
            ? error.message
            : String(error ?? "Unknown error");
      }
      return;
    }

    authStatusEl.textContent = "Opening GitHub login...";
    try {
      const { auth_url, user_code } = await invoke<{
        auth_url: string;
        user_code: string;
        expires_in: number;
        interval: number;
      }>("start_github_login");
      await openUrl(auth_url);
      authStatusEl.textContent = `Enter code ${user_code} in the browser if prompted.`;
      if (statusTimer) {
        window.clearTimeout(statusTimer);
        statusTimer = undefined;
      }
    } catch (error) {
      authStatusEl.textContent =
        error instanceof Error
          ? error.message
          : String(error ?? "Unknown error");
    }
  });

  billingButton?.addEventListener("click", async () => {
    await openUrl(
      "https://github.com/settings/billing/premium_requests_usage?",
    );
  });

  fileButton?.addEventListener("click", async () => {
    const selected = await open({
      multiple: false,
      directory: false,
    });
    if (typeof selected !== "string") return;
    contextPath = selected;
    contextName = selected.split(/[\\/]/).pop() ?? selected;
    if (fileContextEl) {
      fileContextEl.textContent = `Context: ${contextName}`;
      fileContextEl.title = selected;
      fileContextEl.classList.remove("is-hidden");
    }
  });

  const refreshCopilotStatus = async () => {
    try {
      const status = await invoke<{
        installed: boolean;
        version: string | null;
        path: string | null;
      }>("get_copilot_status");
      if (status.installed) {
        const match = status.version?.match(/\d+\.\d+\.\d+/);
        versionEl.textContent = match
          ? `copilot ${match[0]}`
          : "copilot";
        installCopilotButton?.classList.add("is-hidden");
      } else {
        versionEl.textContent = "copilot not installed";
        installCopilotButton?.classList.remove("is-hidden");
      }
    } catch {
      versionEl.textContent = "copilot";
      installCopilotButton?.classList.remove("is-hidden");
    }
  };

  installCopilotButton?.addEventListener("click", async () => {
    installCopilotButton.disabled = true;
    authStatusEl.textContent = "Installing Copilot CLI...";
    try {
      const message = await invoke<string>("install_copilot_cli");
      authStatusEl.textContent = message;
      await refreshCopilotStatus();
      if (message.toLowerCase().includes("installed via winget")) {
        setReloadVisible(true);
      }
    } catch (error) {
      authStatusEl.textContent =
        error instanceof Error
          ? error.message
          : String(error ?? "Copilot install failed.");
    } finally {
      installCopilotButton.disabled = false;
    }
  });

  reloadAppButton?.addEventListener("click", () => {
    window.location.reload();
  });

  void (async () => {
    await updateTokenStatus();
    await refreshCopilotStatus();
  })();

  copyButton.addEventListener("click", async () => {
    if (!lastOutput) return;
    try {
      await navigator.clipboard.writeText(lastOutput);
      authStatusEl.textContent = "Copied to clipboard.";
      copyButton.classList.add("is-copied");
      copyButton.setAttribute("title", "Copied");
      copyButton.setAttribute("aria-label", "Copied");
      const copyTooltip = copyWrap.querySelector(".icon-tooltip");
      if (copyTooltip) copyTooltip.textContent = "Copied";
      if (statusTimer) window.clearTimeout(statusTimer);
      statusTimer = window.setTimeout(() => {
        authStatusEl.textContent = "";
      }, 3_000);
      if (copyFeedbackTimer) window.clearTimeout(copyFeedbackTimer);
      copyFeedbackTimer = window.setTimeout(() => {
        copyButton.classList.remove("is-copied");
        copyButton.setAttribute("title", "Copy output");
        copyButton.setAttribute("aria-label", "Copy output");
        if (copyTooltip) copyTooltip.textContent = "Copy";
      }, 1_500);
    } catch {
      authStatusEl.textContent = "Copy failed.";
      if (statusTimer) window.clearTimeout(statusTimer);
      statusTimer = window.setTimeout(() => {
        authStatusEl.textContent = "";
      }, 3_000);
    }
  });

  historyToggle.addEventListener("click", () => {
    setHistoryVisible(historyEl.hidden);
  });
});
