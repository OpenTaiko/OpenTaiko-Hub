<script>
    // Images
    import optkkunUrl from '$lib/optkkun.png';
    import { _ } from 'svelte-i18n';
</script>

<div class="content">
    <h1>Links</h1>
    <div class="container">
        <ul>
            <h2>{$_('links.section.socials')}</h2>
            <li><a href="https://discord.gg/jvmadKWFe9" target='_blank' class='text-blue-600'>{$_('links.discord_en')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://discord.gg/CJ4nTkpy7t" target='_blank' class='text-blue-600'>{$_('links.discord_ja')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://www.youtube.com/@OpenTaiko" target='_blank' class='text-blue-600'>{$_('links.youtube')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://bsky.app/profile/opentaiko.bsky.social" target='_blank' class='text-blue-600'>{$_('links.bluesky')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://x.com/OpenTaiko" target='_blank' class='text-blue-600'>{$_('links.twitter')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/0auBSQ/OpenTaiko" target='_blank' class='text-blue-600'>{$_('links.source_main')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/OpenTaiko/OpenTaiko-Hub" target='_blank' class='text-blue-600'>{$_('links.source_hub')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/OpenTaiko" target='_blank' class='text-blue-600'>{$_('links.github_org')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
        </ul>
        <img src={optkkunUrl} alt="OpenTaiko-kun" class="image optkkun"/>
        <ul>
            <h2>{$_('links.section.websites')}</h2>
            <li><a href="https://opentaiko.github.io/" target='_blank' class='text-blue-600'>{$_('links.website')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://opentaiko.blogspot.com/" target='_blank' class='text-blue-600'>{$_('links.blog')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://opentaiko.wiki.gg/" target='_blank' class='text-blue-600'>{$_('links.wiki')} <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
        </ul>
    </div>
</div>

<style>
    /* Tailwind 4 compiles component styles in isolation: pull in the app theme for @apply */
    @reference "../../app.css";

    ul {list-style-type: circle;}
    .content {@apply card w-full bg-surface-100-800 p-4;}
    .container {
        display: flex;
        align-items:center;
    }
    img.optkkun {
        display: block;
        margin-left: auto;
        margin-right: auto;
    }
</style>
