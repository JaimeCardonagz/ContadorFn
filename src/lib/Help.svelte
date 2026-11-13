<script lang="ts">
  // Icono "?" que muestra una explicación al pasar el ratón o al enfocarlo.
  // El globo se coloca respecto a la fila (.opcion), para no salirse del panel.
  let { text }: { text: string } = $props();
</script>

<button type="button" class="ayuda" aria-label={text}>?<span class="globo">{text}</span></button>

<style>
  .ayuda {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 18px;
    height: 18px;
    margin-left: 6px;
    padding: 0;
    background: none;
    font-family: inherit;
    flex-shrink: 0;
    border: 1px solid var(--text-muted);
    border-radius: 50%;
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 700;
    line-height: 1;
    cursor: help;
    vertical-align: middle;
  }

  .ayuda:hover,
  .ayuda:focus-visible {
    color: var(--text);
    border-color: var(--primary-soft);
    outline: none;
  }

  .globo {
    display: none;
    position: absolute;
    z-index: 10;
    left: 0;
    right: 0;
    top: calc(100% + 2px);
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: #05050f;
    color: var(--text);
    font-size: 13px;
    font-weight: 400;
    line-height: 1.4;
    letter-spacing: 0;
    text-align: left;
    box-shadow: 0 6px 18px rgb(0 0 0 / 50%);
  }

  .ayuda:hover .globo,
  .ayuda:focus-visible .globo {
    display: block;
  }
</style>
