# Stackel

純粋関数型・スタック指向の汎用言語(仮称)。IO は効果型で表す。

仕様書: 「Stackel 言語仕様書 v0.1」(別ドキュメント)

## 現在の状態 (M5 の途中)

- 字句解析・構文解析・木走査インタプリタ(末尾呼び出しは定数スタック)
- **スタック効果の型検査**(実行前に行い、通らないプログラムは実行されない)
  - 型変数(`a -- a a`)、クォーテーション型(`[Int -- Int]`)
  - 算術・比較などの多重定義は、制約付き型変数で表現(`add` は Int か Float)
  - 署名のない語は推論して一般化。再帰する語には署名が必要
  - `main` は型 `--` でなければならない
- **効果検査**: 語は、実行する効果(`IO`、`Fs`=ファイル、`Env`=コマンドライン引数と環境変数、`Fail`=失敗)を署名に宣言しなければならない
  - 署名のない語は効果も推論される。`apply` はクォーテーション型の効果を実行する
  - **効果多相**: 署名に効果変数(小文字の名前)を書ける。`map :: (List a) [a -- b ! e] -- (List b) ! e` の `e` は、渡したクォーテーションの効果で、そのまま `map` の効果になる(純粋なクォーテーションなら `map` も純粋)
  - **失敗**: `fail :: String -- a ! Fail` が計算を止める。`std.result` の `try` は、失敗するかもしれないクォーテーションを実行して、`Fail` を取り除き、結果を `Result String a` にする
  - **出力の捕捉**: `capture` は、クォーテーションが `print` したものを表示せずに文字列として返す(`IO` を取り除く)
- **代数的データ型と `match`**(再帰型、型パラメータ付き)
- **モジュールとパッケージ**
  - 1ファイル=1モジュール。複数ファイルのプログラムを `use` でつなぐ
  - `pub` を付けた語・型だけが公開される
  - パッケージは `stackel.toml` で宣言し、依存はローカルパスで指定する
  - 依存パッケージに**効果の上限**(`allow`)を付けられる
- **`let`(局所束縛)**: スタックの値に名前を付ける。クォーテーションは名前を取り込む(クロージャ)
- **標準ライブラリ**(Stackel で書いたもの): `std.option` `std.result` `std.list`(`map` `filter` `fold` `range` など)、`std.text`(`split` `join` `lines` `to_int` など)、`std.sys`(`args` `var` `read_file` `write_file`)
- 組み込み語
  - スタック操作、算術・比較・論理
  - 文字列(文字単位): `concat` `length` `show` `substring` `index_of` `ord` `chr` `trim` `upper` `lower` `parse_int` `parse_float`
  - 入出力と終了(`IO`): `print` `read` `eprint` `exit`
  - ファイル(`Fs`): `file_read` `file_write` `file_exists`
  - コマンドライン引数と環境変数(`Env`): `arg_count` `arg` `env_var`
  - 失敗と捕捉: `fail` `capture`(`try` は `std.result`)
  - 失敗しうる語は、値と成否の Bool を残す。`"42" parse_int` は `42 true`、読めないファイルの `file_read` はメッセージと `false`。`std.text` と `std.sys` が `Option` / `Result` で包んだ版を提供する

## 構文の要点

```
# コメントは # から行末まで
module hello
use std.io (print)

type Option a = None | Some a

describe :: (Option Int) -- String
describe = match
  | None "none"
  | Some show

main :: -- ! IO
main = 5 Some describe print
       None describe print
```

- 行頭(1列目)から始まる行が新しい定義。インデントした行は前の定義の続き
- `a b sub` は `a - b`(深い方が第1オペランド)
- ガードは `| then ... | else ...`。`else` は必須で、`else` の中で入れ子にできる
- 型 `ins -- outs ! 効果` は「スタックの上から ins を取り、outs を残し、効果を起こす」
- データ型: `type 名前 パラメータ... = コンストラクタ フィールド型... | ...`
  - コンストラクタは語として使える(`5 Some`)。署名で型を適用するときは括弧を付ける: `(Option Int)`
- `match | C1 本体 | C2 本体 ...` は、データ値のコンストラクタに合う腕を実行する
  - すべてのコンストラクタを1回ずつ書く必要があり、腕は同じスタック効果でなければならない
- `let a b c in 本体` は、スタックの上から3つの値を `a`(一番深い)、`b`、`c`(一番上)に束縛する。本体では、名前がその値を積む語になる
  - 名前は不変で、`let` から本体の終わりまで有効。語と同名の名前は、その範囲で語を隠す
  - 名前は小文字で始め、`.` を含まない。名前の型は単相(一般化しない)
  - `let` は本体の最後に置き、その本体は `match` やガードで終えられる
  - `[ n add ]` のように、クォーテーションは作られた場所の名前を取り込む

## モジュール

```
# src/shapes.stk
use mathx (square)                # パッケージ mathx から square を取り込む

pub type Shape = Circle Int | Square Int

pub area :: Shape -- Int          # pub の語には型署名が必要
pub area = match
  | Circle square 3 mul
  | Square square
```

```
# src/main.stk
use shapes (Shape, Circle, area)  # 選んで取り込む(型とコンストラクタは別々に書く)
use mathx                         # 公開名をすべて取り込む
main = 5 Circle area show print 3 mathx.cube show print   # 修飾名 mathx.cube も使える
```

- `use a.b` のパスは、次の順に探す
  1. `stackel.toml` の依存パッケージ(`use json` は `src/lib.stk`、`use json.decode` は `src/decode.stk`)
  2. 標準ライブラリ `std.*`(`std.io` `std.string` `std.arith` `std.compare` `std.logic` `std.stack`)
  3. 現在のパッケージのモジュール(`use a.b` は `src/a/b.stk`)
- 名前の優先順位は、自分の定義 > `use a (x)` で選んだ名前 > `use a` で取り込んだ名前。
  `use a` と `use b` が同じ名前を取り込んだときは、使った場所でエラーになる(`a.f` と書けば区別できる)
- 型も修飾して書ける(`(opt.Option Int)`、`match` の腕の `opt.None`)。別のモジュールの同名の型は別の型
- `pub` なしの語は他のモジュールから見えない。`import` しようとするとエラーになる
- 標準語は組み込みで、常に使える。`std.io` `std.string` `std.fs` `std.env` `std.arith` `std.compare` `std.logic` `std.stack` はそれらをまとめた名前の集まりで、`use std.io (print)` は必要な語の宣言になる(`std.io.print` と書くこともできる)
- `std.option` `std.result` `std.list` `std.text` `std.sys` は、Stackel で書かれた通常のモジュール(`stdlib/*.stk`。プログラムに埋め込まれている)。`use std.list (range, map, sum)` のように使う。`map` のような同名の語は、修飾名(`std.list.map`)で区別する
- import の循環はエラー
- `module` 行は今のところ説明用で、ファイルの位置とは照合しない
- ファイル単体で実行するときは、そのファイルのあるディレクトリが `src` の代わりになる。
  `stackel.toml` のあるパッケージの中のファイルなら、そのパッケージの `src` を使う

## パッケージ

```toml
# stackel.toml
[package]
name = "app"
version = "0.1.0"
entry = "src/main.stk"         # 省略時の既定値

[deps]
mathx = { path = "../mathx", allow = [] }       # allow = [] なら純粋な語だけ
json  = { path = "../json", allow = ["IO"] }

[deps.parser]                  # 表にも書ける
path = "../parser"
```

- 依存の名前は、そのパッケージの `[package] name` と同じにする
- **効果の上限**: `allow` に書いた効果だけを、その依存の語が実行できる。
  上限を超える語(署名で宣言していても、呼び出しを通じて推論されたものでも)があると、実行前にエラーになる。
  `allow` がなければ制限しない。複数のパッケージから依存されるときは、上限の共通部分になる
- 依存の中のモジュールは、パッケージ名を頭に付けて参照する(`use json.decode`)

## REPL

`stackel repl` で、言語を対話的に試せる。

```
> 1 2 add
stack: 3
> double = dup add

ok
> :type double
double :: a -- a
> 5 double
stack: 3 10
```

- 語を打つと、セッションのスタックの上で実行して、スタックを表示する。エラーのとき、スタックは変わらない
- `double = dup add` のような定義(語、署名、`type`、`use`)は、セッションに残る。字下げした行が続けば続きの行として読み、空行で確定する。同じ名前を定義し直すと置き換わる。検査に通らない定義は捨てる
- `:type <語>`(型を表示)、`:stack` `:clear` `:items` `:reset`、`:load <ファイル>`、`:{` と `:}`(複数行の入力)、`:help`、`:quit`

## テスト

エントリーモジュールの `test_` で始まる語がテストになる。テストは Bool を1つだけ残し、`true` なら合格(`stackel test <ファイル | パッケージ>`)。

```
double = dup add

test_double = 4 double 8 eq
```

```
test_double ... ok
1 tests: 1 passed, 0 failed
```

失敗が1つでもあれば、終了ステータスは 1 になる。型エラーなどは、失敗ではなくエラーとして報告される。エラーには、ソースの該当行と位置の印(`^`)が付く。

## 現在の制限

- 依存はローカルパスだけ。バージョン指定(`json = "^1.2"`)、レジストリ、`stackel.lock`、`use x from "pkg:..."` は未対応
- 再エクスポート、`Option(..)` のような型とコンストラクタの一括 import は未対応
- 明示的な行変数(`..r`)は未対応。クォーテーション型は効果も含めて完全一致が必要
- 効果変数を2つ以上含む集合どうしの単一化は不完全(余分な変数は空に束縛される)。効果の部分集合の関係(純粋な関数を、効果つきの関数が要るところへ渡す)は扱わない
- `try` に渡すクォーテーションは `Fail` を起こしうるものでなければならず、値を1つだけ残す
- `match` にワイルドカード腕はない。`show` と `eq` はデータ値に使えない
- 効果は `IO` `Fs` `Env` `Fail` の4つ。ユーザーが効果を定義する仕組みと `handle` は未対応
- マニフェストの TOML は、文字列・1行の文字列リスト・インライン表だけを読める

## 使い方

```
cargo run -- run examples/hello.stk       # ファイルを実行
cargo run -- run examples/pkg-demo        # パッケージ(stackel.toml のあるディレクトリ)を実行
cargo run -- run examples/stdlib.stk      # 標準ライブラリ(list と option)
cargo run -- run examples/cat.stk README.md   # パスの後ろの引数は、プログラムの引数になる
cargo run -- repl                         # 対話環境
cargo run -- test examples/tests.stk      # test_ で始まる語をテストとして実行
echo "3 4 add 2 mul" | cargo run -- run examples/rpn.stk   # Stackel で書いた RPN 電卓(14)
cargo run -- run                          # カレントディレクトリのパッケージを実行
cargo run -- check examples/option.stk    # 実行せずに検査だけ
cargo run -- --version                    # バージョンを表示
cargo test
```

## インストール

### リリースから

[Releases](../../releases) のページから、使っている環境のアーカイブをダウンロードして展開する(中に `stackel`、README、`examples/` が入っている)。

| OS | アーキテクチャ |
| --- | --- |
| Linux(静的リンク、どのディストリビューションでも動く) | x86_64、aarch64、armv7(hard float)、i686 |
| Linux(glibc) | riscv64、powerpc64le、s390x、loongarch64(実験的) |
| macOS | Apple silicon、Intel |
| Windows | x86_64、i686、arm64 |

アーカイブと同じ場所にある `SHA256SUMS` で、ダウンロードしたファイルを検証できる。

```
sha256sum --check --ignore-missing SHA256SUMS
```

### ソースから

Rust 1.75 以降が必要。

```
cargo install --path .
stackel --version
```

## examples

`examples/` のプログラムは、すべて `cargo test` が実行して、横にある期待出力(`NAME.expected`)と照らし合わせる。出力が変わる変更や、動かなくなる変更はテストで見つかる。

| 分類 | プログラム |
| --- | --- |
| 最初の一歩 | `hello` `fact` `countdown`(末尾呼び出し) `guards` `twice` `stack_ops`(スタック語の一覧) |
| 数と算術 | `fizzbuzz` `fib` `gcd` `power` `primes` `collatz` `hanoi` `ackermann` `even_odd`(相互再帰) `digits` `perfect` `isqrt` `binary`(進数変換) `calendar` `float` `grades` |
| 文字列 | `reverse` `caesar` `roman` `title_case` `wordcount`(標準入力) `csv` |
| データ型とリスト | `list` `option` `stdlib` `sort`(挿入ソートとクイックソート) `tree`(二分探索木) `pascal` `expr`(式評価器) `stack_machine` |
| 効果とエラー処理 | `option_chain` `result_chain` `try_fail` `capture` `effects`(効果多相) `closures` `args`(引数と環境変数) `fs_roundtrip`(ファイル) |
| 道具 | `cat` `rpn`(RPN 電卓) `tests` と `stdlib_tests`(`stackel test`) |
| パッケージ | `pkg-demo` `pkg-effects`(依存に効果の上限を付ける) |

新しい例を足すときは、`examples/NAME.stk` と、出力を確かめたうえで `examples/NAME.expected` を置く(`.expected` がない例があると、テストが失敗する)。必要なら次のファイルも置ける。

- `NAME.stdin`: 標準入力に渡す内容
- `NAME.args`: 引数(1行に1つ)
- `NAME.cmd`: `run`(既定)、`check`、`test` のどれか

パッケージの例は、ディレクトリ `examples/NAME/` と `examples/NAME.expected` の組になる。

## 開発と CI

```
cargo fmt --check                        # 整形
cargo clippy --all-targets -- -D warnings   # 静的解析
cargo test                               # 単体・結合テストと examples
STACKEL_CMD=target/debug/stackel scripts/check-examples.sh   # examples だけを、任意の実行コマンドで
```

GitHub Actions(`.github/workflows/ci.yml`)は、push と pull request で次を実行する。

- **lint**: rustfmt、clippy(`-D warnings`)、shellcheck。結果が変わらないよう、Rust 1.75 で実行する
- **test**: Linux(x86_64、aarch64)、macOS(Apple silicon、Intel)、Windows(x86_64、arm64)の実機で `cargo test`
- **msrv**: Rust 1.75 で `cargo test`
- **cross-test**: i686、aarch64、armv7、riscv64、powerpc64le、s390x(ビッグエンディアン)、loongarch64(実験的)を、`cross` と QEMU で実行
- **cross-build**: FreeBSD、NetBSD、illumos、Windows(gnu)がビルドできること
- **ci-ok**: ブランチ保護に指定するための、まとめのチェック

QEMU の上ではテストバイナリが `stackel` を起動できないことがあるため、そのときは環境変数 `STACKEL_NO_SPAWN=1` で起動を伴うテストを飛ばし、examples は `scripts/check-examples.sh` が確かめる。

## リリース

1. `Cargo.toml` の `version` を上げ、`cargo build` で `Cargo.lock` を更新して、コミットする
2. 試すには、GitHub の Actions で **Release** を手動で実行する(リハーサル)。全プラットフォームをビルドして、アーカイブを workflow の成果物に置くが、公開はしない
3. タグを付けて push する

   ```
   git tag v0.2.0
   git push origin v0.2.0
   ```

タグ(`vMAJOR.MINOR.PATCH`、プレリリースは `v0.2.0-rc.1`)が `Cargo.toml` のバージョンと違うと、ワークフローは最初に止まる。ビルドと検証がすべて通ると、GitHub のリリースに次が公開される。

- 各プラットフォームのアーカイブ(それぞれ、自分の環境かQEMUの上で、全 examples を実行して検証済み)
- `SHA256SUMS`
- リリースノート: 前のタグからのコミットを、[Conventional Commits](https://www.conventionalcommits.org/) の種類(`feat`、`fix`、`perf` ...)ごとにまとめ、`!` か `BREAKING CHANGE` のものを先頭に出す。コミットメッセージが、そのままリリースノートになる

ノートだけをローカルで見るには、`scripts/release-notes.sh v0.2.0`(HEAD でも可)。

## ロードマップ

1. M1: インタプリタ
2. M2: スタック効果の型検査
3. M3: 効果検査、代数的データ型、`match`
4. M4: モジュールとパッケージ解決、効果の上限
5. M5: `let`、Stackel で書く標準ライブラリ、文字列・ファイル・環境の組み込み語(完了)
6. M6: 効果多相、`Fail` / `try`、`handle`(現在。効果多相、`Fail` / `try`、`capture` は実装済み。`handle` は未着手)
7. M7: FFI、レジストリと `stackel.lock`
8. その後: バイトコード VM → JIT(Cranelift)→ セルフホスト
