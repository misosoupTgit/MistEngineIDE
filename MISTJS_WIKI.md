# MistJS 仕様・リファレンス Wiki

MistJS は、MistEngine で動作する QuickJS (JavaScript) をベースに調整されたゲームスクリプト用言語です。
使い慣れた JavaScript の基本構文をそのままに、ゲームループ、描画、入力、数学演算など、ゲーム開発に必要な各種 API があらかじめグローバルにバインドされています。

---

## 1. 基本構造とライフサイクル

ゲームスクリプト内には、エンジンから呼び出される以下のライフサイクル関数を定義します。

### `ready()`
- **説明**: ゲームの開始時に1回だけ呼び出されます。初期化処理（プレイヤーの初期位置設定など）を行います。
- **実装例**:
  ```javascript
  let playerX, playerY;

  function ready() {
      playerX = 100;
      playerY = 150;
      print("Game Ready!");
  }
  ```

### `update(delta)`
- **引数**: `delta` (number - 前フレームからの経過時間、秒単位)
- **説明**: 毎フレーム呼び出される更新処理です。入力の判定や位置の計算などを行います。
- **実装例**:
  ```javascript
  function update(delta) {
      if (input.held("right")) {
          playerX += 200 * delta; // 1秒間に200ピクセル移動
      }
  }
  ```

### `draw()`
- **説明**: 毎フレーム、`update` の後に呼び出される描画用関数です。画面のクリアやキャラクター・UI の描画処理を行います。
- **実装例**:
  ```javascript
  function draw() {
      draw.background("#1a1a1a");
      draw.circle(playerX, playerY, 15, Color.CYAN);
  }
  ```

---

## 2. カラー指定 (`Color` / カラーフォーマット)

描画関数で指定するカラー引数は、以下のいずれの形式でも記述可能です。

1. **RGBA 配列**:
   - `[r, g, b]` または `[r, g, b, a]`
   - 値の範囲が `1.0` を超える要素がある場合は `0〜255`、全て `1.0` 以下の場合は `0.0〜1.0` として自動判定されます。
   - 例: `[255, 0, 0]` (赤), `[0.0, 1.0, 0.0, 0.5]` (半透明の緑)
2. **Hex 文字列**:
   - `"#RGB"`, `"#RRGGBB"`, `"#RRGGBBAA"`
   - 例: `"#f00"`, `"#ff0000"`, `"#00ff0080"`

### `Color` オブジェクトの定数・ヘルパー

| プロパティ / 関数 | 説明 | 返り値 / 例 |
| :--- | :--- | :--- |
| `Color.RED` | 赤色定数 | `[1, 0, 0, 1]` |
| `Color.GREEN` | 緑色定数 | `[0, 1, 0, 1]` |
| `Color.BLUE` | 青色定数 | `[0, 0, 1, 1]` |
| `Color.WHITE` | 白色定数 | `[1, 1, 1, 1]` |
| `Color.BLACK` | 黒色定数 | `[0, 0, 0, 1]` |
| `Color.YELLOW` | 黄色定数 | `[1, 1, 0, 1]` |
| `Color.CYAN` | シアン（水色）定数 | `[0, 1, 1, 1]` |
| `Color.MAGENTA` | マゼンタ定数 | `[1, 0, 1, 1]` |
| `Color.from_hex(hex)` | Hex文字列からカラー配列を生成します。 | `Color.from_hex("#ff5500")` |
| `Color.rgba(r, g, b, a)` | 0〜255 の数値からカラー配列を生成します。 (アルファ値 `a` は省略時 255) | `Color.rgba(255, 128, 0, 200)` |

---

## 3. 描画 API (`draw`)

描画処理は `draw` オブジェクトのメソッドを通じて行います。

| 関数シグネチャ | 説明 |
| :--- | :--- |
| `draw.background(color)` | 画面全体を指定した色で塗りつぶし、背景色を設定します。 |
| `draw.circle(x, y, r, color)` | 中心 `(x, y)`、半径 `r` の円を描画します。 |
| `draw.rect(x, y, w, h, color)` | 左上 `(x, y)`、幅 `w`、高さ `h` の矩形を描画します。 |
| `draw.square(x, y, s, color)` | 中心 `(x, y)`、一辺 `s` の正方形を描画します。 |
| `draw.line(x1, y1, x2, y2, color)` | `(x1, y1)` から `(x2, y2)` まで直線を描画します。 |
| `draw.triangle(x, y, s, color)` | 中心 `(x, y)`、サイズ `s` の正三角形を描画します。 |
| `draw.polygon(x, y, s, sides, color)` | 中心 `(x, y)`、サイズ `s`、角数 `sides` の正多角形を描画します。（最小角数 3） |
| `draw.diamond(x, y, s, color)` | 中心 `(x, y)`、サイズ `s` のひし形（ダイヤ）を描画します。 |
| `draw.text(x, y, text, size, color)` | `(x, y)` にテキストを描画します。（`size` はデフォルトで 24） |
| `draw.image(x, y, path, w, h)` | `(x, y)` に指定したパス `path` の画像を幅 `w`、高さ `h` で描画します。（`w`, `h` を `0` にすると元サイズで描画） |

---

## 4. 入力 API (`input`)

キー入力やアクションの状態を判定します。

| 関数 | 説明 | 返り値 |
| :--- | :--- | :--- |
| `input.action_held(action)`<br>`input.is_action_held(action)`<br>`input.held(action)` | アクション（またはキー名）が現在押されているかを判定します。 | `boolean` |
| `input.action_pressed(action)`<br>`input.pressed(action)` | アクションがそのフレームで新しく押されたかを判定します。 | `boolean` |
| `input.action_released(action)` | アクションがそのフレームで離されたかを判定します。（※簡易実装のため常に `false`） | `boolean` |

---

## 5. 数学関数 (`math`)

ゲーム演算に便利な数学関連の定数とメソッドです。

### 定数
- `math.PI`: 円周率 ($\approx 3.14159$)
- `math.TAU`: 2倍円周率 ($\approx 6.28318$)
- `math.E`: ネイピア数 ($\approx 2.71828$)
- `math.INF`: 無限大 (`Infinity`)

### メソッド

| 関数 | 説明 |
| :--- | :--- |
| `math.sin(x)` | 正弦（サイン）。ラジアン単位。 |
| `math.cos(x)` | 余弦（コサイン）。ラジアン単位。 |
| `math.tan(x)` | 正接（タンジェント）。ラジアン単位。 |
| `math.sqrt(x)` | 平方根。 |
| `math.abs(x)` | 絶対値。 |
| `math.floor(x)` | 切り捨て。 |
| `math.ceil(x)` | 切り上げ。 |
| `math.round(x)` | 四捨五入。 |
| `math.log(x)` | 自然対数。 |
| `math.sign(x)` | 符号（正なら 1、負なら -1、ゼロなら 0）。 |
| `math.pow(x, y)` | $x$ の $y$ 乗。 |
| `math.max(x, y)` | 大きい方の値を返します。 |
| `math.min(x, y)` | 小さい方の値を返します。 |
| `math.clamp(x, lo, hi)` | $x$ を `lo` 以上 `hi` 以下の範囲に収めます。 |
| `math.lerp(a, b, t)` | $a$ と $b$ の間を $t$ ($0.0 \sim 1.0$) で線形補間します。 |
| `math.rand()` | $0.0$ 以上 $1.0$ 未満の乱数を生成します（高速な xoshiro256 を使用）。 |
| `math.rand_int(lo, hi)` | `lo` 以上 `hi` 未満のランダムな整数を返します。 |
| `math.atan2(y, x)` | 逆正接（アークタンジェント）。 |
| `math.hypot(x, y)` | $\sqrt{x^2 + y^2}$ を計算します。 |

---

## 6. エンジン情報 API (`engine`)

ゲームエンジンのステータスや画面サイズを取得します。

| 関数 | 説明 | 返り値の例 |
| :--- | :--- | :--- |
| `engine.fps()` | 現在のリアルタイム FPS を取得します。 | `60` |
| `engine.width()` | ゲーム画面（ウィンドウ）の幅を取得します。 | `800` |
| `engine.height()` | ゲーム画面（ウィンドウ）の高さを取得します。 | `600` |

---

## 7. グローバルユーティリティ

スクリプト内のどこからでも直接呼び出せるグローバルな関数群です。

### デバッグ出力
- `print(...args)`
  - コンソール画面およびシステム標準出力に文字列を出力します。
  - 複数引数はスペース区切りで連結されます。
- `debug(...args)`
  - システムのデバッグ標準エラー出力に `[debug]` プレフィックス付きで出力します。

### 移動・角度計算（Mistral 互換）
- `rotate(current, delta)`
  - 現在の角度 `current` に `delta` を加え、`0〜360` の範囲に正規化した角度を返します。
- `move_forward(x, y, steps, angle)`
  - 座標 `(x, y)` から、指定した角度 `angle` 方向（`0`が上、`90`が右、`180`が下、`270`が左。Scratch互換）に `steps` 歩進んだ新しい座標を `[new_x, new_y]` の配列で返します。
  - `angle` のデフォルト値は `90`（右方向）です。

### 型変換
- `str(v)`: 文字列型に変換します。
- `int(v)`: 整数型に変換します（小数点以下切り捨て）。
- `float(v)`: 浮動小数点数型に変換します。
- `bool(v)`: 真偽値型に変換します。

### その他
- `wait(secs)`
  - ※ゲームループが時間を管理するため、何も処理を行わない（No-Op）関数です。

---

## 8. プロジェクト設定ファイル (`project.json`)

プロジェクトのルートディレクトリに配置し、画面サイズや描画オプション、起動ファイルなどの基本設定を行う JSON ファイルです。

### 設定項目一覧

| キー名 | 型 | デフォルト値 | 説明 |
| :--- | :--- | :--- | :--- |
| `name` | `string` | `"MyGame"` | プロジェクトの名前。 |
| `version` | `string` | `"0.1.0"` | プロジェクトのバージョン。 |
| `window_width` | `number` | `1280` | ウィンドウの初期幅（ピクセル）。 |
| `window_height` | `number` | `720` | ウィンドウの初期高さ（ピクセル）。 |
| `resizable` | `boolean` | `true` | ウィンドウをリサイズ可能にするかどうか。 |
| `high_dpi` | `boolean` | `true` | 高解像度ディスプレイ（DPI拡大）時に内部バッファを自動的に拡大して画質を向上（ダウンサンプリング）させるか。 |
| `anti_alias` | `number` | `2.0` | SSAA（スーパーサンプリング・アンチエイリアス）の倍率。 (1.0 = オフ, 2.0 = 2×SSAA, 4.0 = 4×SSAA) |
| `vsync` | `boolean` | `true` | 垂直同期を有効にし、60fps上限とするかどうか。 |
| `main_file` | `string` | `"main.js"` | ゲームのメインエントリファイル。 |

### 設定例 (`project.json`)
```json
{
  "name": "MyGame",
  "version": "0.1.0",
  "window_width": 1280,
  "window_height": 720,
  "resizable": true,
  "high_dpi": true,
  "anti_alias": 2.0,
  "vsync": true,
  "main_file": "main.js"
}
```

---

## 9. 入力設定ファイル (`input.json`)

キーボードやゲームコントローラーのボタンを仮想的な「アクション名」にバインドするための JSON ファイルです。
ゲーム側からは、ここで定義したアクション名を指定して `input.held("action_name")` のように呼び出します。

### 設定構造
トップレベルに `"keys"` オブジェクトを置き、キーを「アクション名」、値に「バインドしたい物理入力の文字列の配列」を指定します。

### 設定例 (`input.json`)
```json
{
  "keys": {
    "move_up":    ["Key.W", "Key.Up",    "Controller.DPad.Up"],
    "move_down":  ["Key.S", "Key.Down",  "Controller.DPad.Down"],
    "move_left":  ["Key.A", "Key.Left",  "Controller.DPad.Left"],
    "move_right": ["Key.D", "Key.Right", "Controller.DPad.Right"],
    "jump":       ["Key.Space",          "Controller.A"],
    "attack":     ["Key.Z",              "Controller.X"],
    "pause":      ["Key.Escape",         "Controller.Start"]
  }
}
```

### 使用可能な物理入力文字列

#### 1. キーボード (`Key.X` 形式)
- **アルファベット**: `Key.A` 〜 `Key.Z`
- **キーボード上部数字**: `Key.Num0` 〜 `Key.Num9` （※内部実装上は定義されていますが、マッチングは一部簡略化される場合があります）
- **カーソルキー**: `Key.Up`, `Key.Down`, `Key.Left`, `Key.Right`
- **特殊キー**: `Key.Space`, `Key.Enter`, `Key.Escape`, `Key.Tab`, `Key.Shift`, `Key.Ctrl`, `Key.Alt`
- **ファンクションキー**: `Key.F1` 〜 `Key.F12`

#### 2. コントローラー (XInput対応 / `Controller.X` 形式)
- **前面ボタン**: `Controller.A`, `Controller.B`, `Controller.X`, `Controller.Y`
- **ショルダー/トリガー**: `Controller.LB`, `Controller.RB`, `Controller.LT`, `Controller.RT`
- **システムボタン**: `Controller.Start`, `Controller.Back`
- **方向パッド (D-Pad)**: `Controller.DPad.Up`, `Controller.DPad.Down`, `Controller.DPad.Left`, `Controller.DPad.Right`

---

## 10. オブジェクト指向機能 (`GameObject` システム)

MistJS は、手続き型や関数型の自由な書き方を崩さずに、オブジェクト指向（OOP）的な設計を導入できる軽量なゲームオブジェクトシステムを提供します。

### `GameObject` クラス

ゲーム内のエンティティ（プレイヤー、敵、アイテム、弾など）の基底クラスです。継承してカスタムクラスを定義することができます。

#### プロパティ

| プロパティ名 | 型 | 初期値 | 説明 |
| :--- | :--- | :--- | :--- |
| `x` | `number` | `0` | オブジェクトのX座標。 |
| `y` | `number` | `0` | オブジェクトのY座標。 |
| `w` | `number` | `0` | 当たり判定に使用する幅。 |
| `h` | `number` | `0` | 当たり判定に使用する高さ。 |
| `active` | `boolean` | `true` | オブジェクトのアクティブフラグ。`false` になると `update_objects()` 時のループから自動的に除外（クリーンアップ）されます。 |
| `visible` | `boolean` | `true` | 表示フラグ。`false` にすると `draw_objects()` 時の自動描画からスキップされます。 |

#### メソッド

| メソッド名 | 説明 |
| :--- | :--- |
| `constructor(x, y)` | 座標を指定してインスタンスを作成します。作成されたインスタンスは自動的に `engine` の管理下に追加されます。 |
| `update(delta)` | 状態の更新処理を行います。継承先でオーバーライドします。 |
| `draw()` | 描画処理を行います。継承先でオーバーライドします。 |
| `destroy()` | オブジェクトを破棄します。アクティブフラグを `false` にし、`engine` の管理下から削除します。 |
| `collides_with(other)` | 別の `GameObject` インスタンスとの矩形（AABB）による当たり判定を行い、衝突していれば `true` を返します。 |

---

### `engine` オブジェクト管理 API

生成された `GameObject` インスタンスは、`engine` に備わっているマネージャーによって自動的に一括管理されます。

| 関数 | 説明 |
| :--- | :--- |
| `engine.add_object(obj)` | 指定したオブジェクトを手動で管理対象に追加します。（通常は `new GameObject()` 時に自動追加されるため、手動で呼ぶ必要はありません） |
| `engine.remove_object(obj)` | 指定したオブジェクトを手動で管理対象から削除します。 |
| `engine.clear_objects()` | 管理対象のすべてのオブジェクトを削除します。 |
| `engine.get_objects()` | 現在管理されているすべてのオブジェクトの配列を返します。 |
| `engine.update_objects(delta)` | 管理対象のすべての `active` なオブジェクトの `update(delta)` を呼び出します。また、`active` が `false` になったオブジェクトを自動で管理リストから除外します。 |
| `engine.draw_objects()` | 管理対象のすべての `active` かつ `visible` なオブジェクトの `draw()` を呼び出して一括描画します。 |

