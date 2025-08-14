<script>
    // Images
    import optkkunUrl from '$lib/optkkun.png';
</script>

<div class="content">
    <h1>Links</h1>
    <div class="container">
        <ul>
            <h2>Socials/GitHub</h2>
            <li><a href="https://discord.gg/jvmadKWFe9" target='_blank' class='text-blue-600'>Official English Discord Server <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://discord.gg/CJ4nTkpy7t" target='_blank' class='text-blue-600'>Official Japanese Discord Server <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://www.youtube.com/@OpenTaiko" target='_blank' class='text-blue-600'>Official YouTube Channel <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://bsky.app/profile/opentaiko.bsky.social" target='_blank' class='text-blue-600'>Official Bluesky Account <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://x.com/OpenTaiko" target='_blank' class='text-blue-600'>Official Twitter/X Account <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/0auBSQ/OpenTaiko" target='_blank' class='text-blue-600'>Main OpenTaiko Source Code <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/OpenTaiko/OpenTaiko-Hub" target='_blank' class='text-blue-600'>OpenTaiko Hub Source Code <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://github.com/OpenTaiko" target='_blank' class='text-blue-600'>GitHub Organization <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
        </ul>
        <img src={optkkunUrl} alt="OpenTaiko-kun" class="image optkkun"/>
        <ul>
            <h2>Websites</h2>
            <li><a href="https://opentaiko.github.io/" target='_blank' class='text-blue-600'>Official OpenTaiko Website <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
            <li><a href="https://opentaiko.wiki.gg/" target='_blank' class='text-blue-600'>Official OpenTaiko Wiki <i class="fa-solid fa-arrow-up-right-from-square"></i></a><br></li>
        </ul>
    </div>
</div>

<style>
    .container {
        display: flex;
        align-items:center;
    }
    .content {@apply card w-full bg-surface-100-800-token p-4;}
    ul {
        list-style-type: circle;
    }
    img.optkkun {
        display: block;
        margin-left: auto;
        margin-right: auto;
    }
</style>