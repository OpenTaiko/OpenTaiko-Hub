<script lang="ts">
    interface Props {
        Url?: string;
        ImageSrc?: string;
        CardTitle?: string;
        CardSubtitle?: string;
        CardText?: string;
    }

    let {
        Url = "",
        ImageSrc = "",
        CardTitle = "",
        CardSubtitle = "",
        CardText = ""
    }: Props = $props();
</script>

<a class="card bg-initial overflow-hidden no-underline" href={Url} target="_blank">
    <header>
        <img src={ImageSrc} class="bg-black/50 w-full aspect-[21/9] object-contain" alt="Post">
    </header> 
    <div class="p-4 space-y-4 text-black dark:text-white">
        {#if CardTitle}<h2 data-toc-ignore="">{CardTitle}</h2>{/if}
        <h6 data-toc-ignore="">{CardSubtitle}</h6>
        <article>
            <p>{CardText}
            </p>
        </article>
    </div> 
</a>

<style>
</style>