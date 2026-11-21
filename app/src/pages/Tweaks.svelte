<script lang="ts">
  import { t } from "../lib/state.svelte";
</script>

<div class="page">
  <h1>{t("app.tweaks.title")}</h1>
  <p class="muted">{t("app.soon")}</p>
</div>

<style>
  .page {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  p {
    margin: 0;
  }
</style>
