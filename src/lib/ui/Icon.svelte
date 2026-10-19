<script lang="ts">
  import { icons, type IconName } from './icons';

  interface Props {
    name: IconName;
    size?: number;
    class?: string;
  }

  let { name, size = 16, class: className = '' }: Props = $props();
</script>

<svg
  class="icon {className}"
  width={size}
  height={size}
  viewBox="0 0 256 256"
  fill="currentColor"
  aria-hidden="true"
  focusable="false"
>
  {@html icons[name]}
</svg>

<style>
  .icon {
    display: block;
    flex: none;
  }
</style>
