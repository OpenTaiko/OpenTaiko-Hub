<script lang="ts">
    import "../app.css";
    import "../lite-yt-embed.css";
    import "../lite-yt-embed.js";

    import { onMount, setContext, type Snippet } from 'svelte';
    import { Toast } from '@skeletonlabs/skeleton-svelte';
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

    onMount(async () => {
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
