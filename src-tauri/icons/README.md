# DisplayFlip 앱 아이콘

`icon-source.png`는 기본 내장 imagegen 도구로 제작한 원본입니다. 청록색 타일 위 모니터 두 대와 양방향 화살표로 입력 전환 기능을 표현합니다. 모서리 바깥은 투명하며, 앱의 청록·파랑 색상과 맞췄습니다.

Mac ICNS, Windows ICO, PNG 크기별 파일은 Tauri 도구로 생성합니다.

```sh
pnpm tauri icon src-tauri/icons/icon-source.png --output src-tauri/icons
```

명령이 함께 생성하는 `android/`, `ios/`는 현재 데스크톱 앱에서 사용하지 않습니다. 앱 상단·웹 아이콘의 `public/displayflip-icon.png`는 생성된 `128x128.png`를 복사한 파일입니다.

## 제작 프롬프트

Use case: logo-brand. Create ONE production desktop app icon for DisplayFlip, a utility switching two external monitors between Mac and Windows. Square 1024x1024 icon asset, transparent outside an inset rounded-square tile with generous 8% outside margin. Background tile dark deep teal #17333c, gentle dimensional bevel and restrained soft lighting, premium native macOS utility icon. Center a bold, extremely clean ivory graphic of TWO small monitor silhouettes arranged diagonally, one upper left and one lower right, joined by two opposing curved switching arrows. Screens use flat light mint and pale blue accents inspired by the actual DisplayFlip UI #285bd5. Minimal geometric forms, large readable shapes, crisp edges, uniform thick strokes, visually balanced, recognizable down to 32 pixels. No letters, no text, no logos of operating systems, no watermark, no extra icons, no mockup or desktop scene. Fill most of the tile with the simple symbol, no busy detail. Actual alpha transparency outside the rounded tile.
