// Unsaved edits that switching away would throw away. The folder settings form sets
// this while it holds changes; the Songs page asks before changing the selection.
import { writable, get } from 'svelte/store';
import { confirm } from '@tauri-apps/plugin-dialog';
import { _ } from 'svelte-i18n';

export const unsavedBoxDef = writable(false);

/** True when nothing is lost by moving on, or the user agreed to discard the changes. */
export const ConfirmDiscardEdits = async (): Promise<boolean> => {
    if (!get(unsavedBoxDef)) return true;
    const t = get(_);
    const discard = await confirm(t('songs.box.discard_message'), {
        title: t('songs.box.discard_title'),
        kind: 'warning',
        okLabel: t('songs.box.discard'),
        cancelLabel: t('common.cancel')
    });
    if (discard) unsavedBoxDef.set(false);
    return discard;
};
