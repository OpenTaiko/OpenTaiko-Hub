<script lang="ts">
    import "../app.css";
    import "../lite-yt-embed.css";
    import "../lite-yt-embed.js";

    import { onMount, onDestroy, setContext, type Snippet } from 'svelte';
    import { Toast } from '@skeletonlabs/skeleton-svelte';
    import { ipc } from '$lib/ipc';
    import { error as logError, warn as logWarn } from '@tauri-apps/plugin-log';
    import { download } from '$lib/utils/download';
    import { readTextFile } from '@tauri-apps/plugin-fs';
    import { path } from '@tauri-apps/api';

    import { setupI18n } from '$lib/i18n';
    import { _, isLoading, locale } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { GetPreferencesPath } from '$lib/utils/path';
    import { toaster } from '$lib/utils/toaster';
    import { SafeHtml } from '$lib/utils/sanitize';
    import { isCancelled } from '$lib/stores/activity';
    import { DescribeError, ErrorText } from '$lib/utils/errors';
    import type { ErrorToastOptions, ProgressPayload, ToastContext } from '$lib/types';

    interface Props {
        children?: Snippet;
    }

    let { children }: Props = $props();

    setupI18n();

    // The page language follows the Hub's: screen readers read with the right voice, and
    // for Chinese, Japanese and Korean text the system picks the matching glyph variants
    // when it falls back from the themes' Latin-only fonts. It was always "en".
    locale.subscribe((value) => {
        if (value) document.documentElement.lang = value;
    });

    // Errors stay until dismissed: they often carry what to do next, and a report needs
    // their text, which used to vanish after four seconds. The copy button takes the raw
    // details when the message is a friendlier rewording of them.
    const TriggerError = (msg: string, options: ErrorToastOptions = {}) => {
        console.error(options.details ? `${msg} (${options.details})` : msg);
        toaster.error({
            description: msg,
            duration: Infinity,
            meta: { details: options.details ?? PlainText(msg), action: options.action }
        });
    };

    // Toast messages carry markup (<br>, <b>), which is not wanted in copied text
    const PlainText = (html: string): string => {
        const element = document.createElement('div');
        element.innerHTML = SafeHtml(html.replace(/<br\s*\/?>/gi, '\n'));
        return element.textContent ?? html;
    };

    let copiedToast = $state<string | null>(null);
    const CopyDetails = async (toastId: string, text: string) => {
        try {
            await navigator.clipboard.writeText(text);
        } catch {
            // Older webviews: fall back to a selection copy
            const area = document.createElement('textarea');
            area.value = text;
            document.body.appendChild(area);
            area.select();
            document.execCommand('copy');
            area.remove();
        }
        copiedToast = toastId;
        setTimeout(() => { if (copiedToast === toastId) copiedToast = null; }, 1500);
    };

    const TriggerWarning = (msg: string) => {
        console.warn(msg);
        toaster.warning({ description: msg, duration: 4000 });
    };

    const TriggerSuccess = (msg: string) => {
        console.log(msg);
        toaster.success({ description: msg, duration: 4000 });
    };

    const wait = (ms: number): Promise<void> => {
        return new Promise(resolve => setTimeout(resolve, ms));
    }

    // Retries happen in the backend, with an increasing delay, and only for failures
    // another attempt can fix: a missing file fails at once instead of ten times over.
    // They are logged rather than announced, so a flaky connection does not bury the
    // screen in warnings; the user hears about the download only if it gives up.
    const backoffDownload = async (
        url: string,
        path: string,
        prfunc?: (progress: ProgressPayload) => void,
        attempts?: number,
        job?: string
    ): Promise<boolean> => {
        try {
            await download(url, path, prfunc, attempts, job);
            return true;
        } catch (error) {
            // A cancellation is the caller's to report
            if (isCancelled(error)) throw error;
            console.error(`Download of ${url} failed:`, error);
            TriggerError(`${get(_)('dl.error.failed_abort')}<br>${DescribeError(error)}`, { details: `${url}: ${ErrorText(error)}` });
            return false;
        }
    }

    const toastContext: ToastContext = { TriggerError, TriggerWarning, TriggerSuccess, backoffDownload, wait };
    setContext('toast', toastContext);

    // Shipped builds have devtools compiled out, so anything written to the console is
    // invisible on a user's machine. Mirror the problem levels into the backend log file
    // (the plugin writes to the log, never back to the console, so this cannot loop).
    const captureFrontendProblems = () => {
        const asText = (values: unknown[]) => values.map((v) => {
            if (v instanceof Error) return `${v.name}: ${v.message}
${v.stack ?? ''}`;
            if (typeof v === 'string') return v;
            try { return JSON.stringify(v); } catch { return String(v); }
        }).join(' ');

        const originalError = console.error;
        const originalWarn = console.warn;
        console.error = (...values: unknown[]) => {
            logError(asText(values)).catch(() => {});
            originalError.apply(console, values as []);
        };
        console.warn = (...values: unknown[]) => {
            logWarn(asText(values)).catch(() => {});
            originalWarn.apply(console, values as []);
        };

        window.addEventListener('error', (event) => {
            logError(`Uncaught ${event.message} at ${event.filename}:${event.lineno}:${event.colno}
${event.error?.stack ?? ''}`).catch(() => {});
        });
        window.addEventListener('unhandledrejection', (event) => {
            const reason = event.reason;
            logError(`Unhandled rejection: ${reason?.stack ?? reason?.message ?? String(reason)}`).catch(() => {});
        });
    };

    // Tells the backend the page is still alive. When these stop while the app is
    // running, the page process has died and the window is blank — the backend logs that
    // so a bug report carries a cause instead of just "it crashed".
    let heartbeat: ReturnType<typeof setInterval> | null = null;
    const sendHeartbeat = () => { ipc.uiHeartbeat().catch(() => {}); };

    onDestroy(() => {
        if (heartbeat !== null) clearInterval(heartbeat);
    });

    onMount(async () => {
        captureFrontendProblems();
        sendHeartbeat();
        heartbeat = setInterval(sendHeartbeat, 5000);

        // Restore persisted locale
        try {
            const prefsDir = await GetPreferencesPath();
            const langFilePath = await path.join(prefsDir, 'language.json');
            const langFile = await readTextFile(langFilePath);
            const { locale: savedLocale } = JSON.parse(langFile) as { locale?: string };
            if (savedLocale) locale.set(savedLocale);
        } catch {
            // First launch or missing file — getLocaleFromNavigator() already applied
        }
    })
</script>

<Toast.Group {toaster}>
    {#snippet children(toast)}
        <Toast {toast}>
            <Toast.Message>
                {#if toast.title}<Toast.Title>{toast.title}</Toast.Title>{/if}
                <!-- Messages are app translations that carry markup (<br>, <b>), which the
                     Skeleton v2 toast used to render as HTML. Values filled into them
                     (error text, file names) can come from outside, hence SafeHtml -->
                <Toast.Description>{@html SafeHtml(toast.description)}</Toast.Description>
                {#if toast.type === 'error'}
                    <div class="toast-actions">
                        {#if toast.meta?.action}
                            <button type="button" class="button-blue button-main" onclick={() => toast.meta?.action.run()}>{toast.meta.action.label}</button>
                        {/if}
                        <button type="button" class="button-gray button-main" onclick={() => CopyDetails(toast.id ?? '', toast.meta?.details ?? '')}>
                            <i class="fa-regular {copiedToast === toast.id ? 'fa-circle-check' : 'fa-copy'}"></i>
                            {copiedToast === toast.id ? $_('toast.copied') : $_('toast.copy')}
                        </button>
                    </div>
                {/if}
            </Toast.Message>
            <Toast.CloseTrigger />
        </Toast>
    {/snippet}
</Toast.Group>
{#if !$isLoading}
{@render children?.()}
{/if}

<style>
    .toast-actions {
        display: flex;
        gap: 0.5rem;
        margin-top: 0.5rem;
    }
</style>
