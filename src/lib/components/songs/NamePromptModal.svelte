<script lang="ts">
    // Asks for one name. This exists because window.prompt renders the page origin
    // ("localhost:1420 says…") in the dialog's own chrome, which is meaningless to
    // someone who only knows they clicked "New folder".
    //
    // Validation is live rather than on submit, so a name that is already taken is
    // refused before the folder is created rather than by a toast afterwards.
    import { _ } from 'svelte-i18n';
    import Modal from '$lib/components/Modal.svelte';

    interface Props {
        Title: string;
        Label: string;
        Placeholder?: string;
        Confirm: string;
        /** Lower-cased names that already exist at this level. */
        Taken?: Set<string>;
        OnConfirm: (name: string) => void;
        OnCancel: () => void;
    }

    let { Title, Label, Placeholder = '', Confirm, Taken, OnConfirm, OnCancel }: Props = $props();

    let value = $state('');
    let trimmed = $derived(value.trim());
    let clash = $derived(trimmed.length > 0 && (Taken?.has(trimmed.toLowerCase()) ?? false));
    let valid = $derived(trimmed.length > 0 && !clash);

    const Submit = () => {
        if (valid) OnConfirm(trimmed);
    };

    // Autofocus without the a11y warning the attribute carries.
    const focus = (node: HTMLInputElement) => node.focus();
</script>

<Modal Label={Title} OnClose={OnCancel}>
    <div class="prompt">
        <h2>{Title}</h2>

        <label class="field">
            <span>{Label}</span>
            <input
                type="text"
                bind:value
                placeholder={Placeholder}
                use:focus
                onkeydown={(e) => { if (e.key === 'Enter') { e.preventDefault(); Submit(); } }}
            >
        </label>

        <!-- Reserved whether or not there is a message, so the buttons do not jump. -->
        <p class="note" class:bad={clash}>
            {#if clash}{$_('songs.folder.name_taken', { values: { name: trimmed } })}{/if}
        </p>

        <div class="buttons">
            <button type="button" class="button-gray button-main" onclick={() => OnCancel()}>
                {$_('common.cancel')}
            </button>
            <button type="button" class="button-green button-main" disabled={!valid} onclick={Submit}>
                {Confirm}
            </button>
        </div>
    </div>
</Modal>

<style>
    .prompt {
        width: min(26rem, calc(100vw - 2rem));
        display: flex;
        flex-direction: column;
        gap: 0.5rem;
        padding: 1rem 1.1rem;
    }
    .prompt h2 {margin: 0; font-size: 1.05rem;}

    .field {display: flex; flex-direction: column; gap: 0.25rem;}
    .field span {font-size: 0.78rem; opacity: 0.75;}
    .field input {
        font: inherit;
        padding: 0.4rem 0.6rem;
        border-radius: 0.375rem;
        border: 1px solid color-mix(in oklab, var(--color-surface-500) 40%, transparent);
        background-color: light-dark(var(--color-surface-50), var(--color-surface-900));
        color: light-dark(var(--color-surface-950), var(--color-surface-50));
        outline: none;
    }
    .field input:focus {border-color: var(--color-primary-500);}

    .note {min-height: 1.1rem; margin: 0; font-size: 0.75rem;}
    .note.bad {color: light-dark(#b91c1c, #fca5a5);}

    .buttons {display: flex; justify-content: flex-end; gap: 0.4rem;}
</style>
