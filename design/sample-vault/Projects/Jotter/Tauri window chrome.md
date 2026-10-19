Jotter draws its own title bar so tabs sit at the top of the window on every platform. Part of [[Design overhaul]].

## Per platform

| Platform | Config | Window controls |
| --- | --- | --- |
| Windows | `decorations: false` | Drawn by Jotter |
| Linux | `decorations: false` | Drawn by Jotter |
| macOS | `titleBarStyle: "Overlay"` | Native traffic lights |

## macOS config

```json
{
  "app": {
    "windows": [
      { "titleBarStyle": "Overlay", "hiddenTitle": true }
    ]
  }
}
```

## Notes

- Mark the tab strip with `data-tauri-drag-region` so empty space drags the window.
- Double-click on the drag region maximizes the window.
- [ ] Snap layout flyout on hover over maximize (Windows 11)
- [ ] Test the drag region with a single tab open
