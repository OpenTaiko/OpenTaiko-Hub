<script lang="ts">
    import "../app.css";
    import "../lite-yt-embed.css";
    import "../lite-yt-embed.js";

    import { onMount, onDestroy, setContext, type Snippet } from 'svelte';
    import { Toast } from '@skeletonlabs/skeleton-svelte';
    import { invoke } from '@tauri-apps/api/core';
    import { error as logError, warn as logWarn } from '@tauri-apps/plugin-log';
    import { download } from "@tauri-apps/plugin-upload";
    import { readTextFile } from '@tauri-apps/plugin-fs';
    import { path } from '@tauri-apps/api';

    import { setupI18n } from '$lib/i18n';
    import { _, isLoading, locale } from 'svelte-i18n';
    import { get } from 'svelte/store';

    import { GetPreferencesPath } from '$lib/utils/path';
    import { toaster } from '$lib/utils/toaster';
    import type { ProgressPayload, ToastContext } from '$lib/types';

    interface Props {
        children?: Snippet;
    }

    let { children }: Props = $props();

    setupI18n();

    const TriggerError = (msg: string) => {
        console.error(msg);
        toaster.error({ description: msg, duration: 4000 });
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

    const backoffDownload = async (
        url: string,
        path: string,
        prfunc?: (progress: ProgressPayload) => void,
        n = 10,
        t = 1000
    ): Promise<boolean> => {
        for (let attempt = 1; attempt <= n; attempt++) {
            try {
                await download(url, path, prfunc);
                return true;
            }
            catch (error) {
                if (attempt === n) {
                    TriggerError(get(_)('dl.error.failed_abort'));
                    return false;
                }
                TriggerWarning(get(_)('dl.warn.failed_retry', { values: { attempt, max: n } }));

                await wait(t);
            }
        }
        return false;
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
    const sendHeartbeat = () => { invoke('ui_heartbeat').catch(() => {}); };

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
                     Skeleton v2 toast used to render as HTML -->
                <Toast.Description>{@html toast.description}</Toast.Description>
            </Toast.Message>
            <Toast.CloseTrigger />
        </Toast>
    {/snippet}
</Toast.Group>
{#if !$isLoading}
{@render children?.()}
{/if}
