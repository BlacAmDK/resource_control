# 服务器资源占用程序

为了保持服务器的 CPU 与内存使用率，通过此程序在不影响服务器其他程序运行的情况下将 CPU 与内存占用率控制在一定范围内。

## 运行逻辑

1. **CPU 控制**: 根据 CPU 核心数，在每个核心各建立一个线程，每个线程根据当前核心使用率以 200ms 为单位，通过选择计算和休眠两种状态将 CPU 使用率保持在目标值（默认 55%）。
2. **内存控制**: 以 200ms 为单位循环计算内存使用率，如果低于最小值则申请并占用内存；在目标范围内时不做任何操作；高于最大值时释放占用的内存，默认控制在45%-55%。

## 使用方法

### 默认运行

```bash
cargo run --release
```

### 自定义参数

```bash
# 设置 CPU 目标为 60%，内存范围 40-60%
cargo run --release -- --cpu-target 60 -l 40 -u 60

# 启用详细日志
cargo run --release -- --verbose

# 查看帮助
cargo run --release -- --help
```

### CLI 参数

| 参数 | 描述 | 默认值 |
|------|------|--------|
| `--cpu-target` | CPU 目标使用率 (0-100) | 55 |
| `-l`, `--ram-min` | 最小内存使用率 (0-100) | 45 |
| `-u`, `--ram-max` | 最大内存使用率 (0-100) | 55 |
| `-v`, `--verbose` | 启用详细日志 | false |

### 后台运行

```bash
nohup ./target/release/resource_control &
```

## 开发

### 构建

```bash
cargo build --release
```

### 测试

```bash
cargo test              # 运行所有测试
cargo test <name>       # 运行指定测试
```

### 代码检查

```bash
cargo clippy           # Linter
cargo fmt              # 格式化
```
