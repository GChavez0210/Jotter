<script lang="ts">
  import QuickCapture from '../../src/lib/QuickCapture.svelte';
  import { presetProfile } from '../../src/lib/captureProfiles';
  import { captureControl, settingsControl } from '../ipc.mock';
  settingsControl.stored = JSON.stringify({
    'jotter.themePref': 'dark',
    'jotter.captureProfiles': JSON.stringify([presetProfile('task', 'Task', 'task')]),
  });
  Object.assign(window, { __review: { captureControl, settingsControl } });
</script>
<QuickCapture />
