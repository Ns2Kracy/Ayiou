# Ayiou WASM Plugin - TypeScript/AssemblyScript Example

这是一个使用 [AssemblyScript](https://www.assemblyscript.org/) 编写的 Ayiou WASM 插件示例。

AssemblyScript 是 TypeScript 的一个子集，可以直接编译为 WebAssembly。

## 功能

这个示例插件响应以下命令：

- `/hello` - 向用户问好
- `/hello <name>` - 向指定名字问好
- `/hi` - 简短问候

## 构建

```bash
# 安装依赖
npm install

# 构建 WASM 文件
npm run build
```

构建完成后，WASM 文件位于 `build/plugin.wasm`。

## 使用

将生成的 `build/plugin.wasm` 复制到 Ayiou 的插件目录，或通过配置文件加载：

```toml
# plugins.toml
[[plugins]]
name = "hello-ts"
enabled = true
source = "./plugins/plugin.wasm"
```

## 项目结构

```
wasm-plugin-ts/
├── assembly/
│   ├── index.ts      # 插件主代码
│   └── tsconfig.json # AssemblyScript 配置
├── build/
│   └── plugin.wasm   # 编译输出
├── asconfig.json     # AssemblyScript 编译配置
├── package.json
└── README.md
```

## 插件 ABI

Ayiou WASM 插件需要导出以下函数：

| 函数            | 签名                        | 说明                         |
|-----------------|-----------------------------|------------------------------|
| `ayiou_meta`    | `() -> i32`                 | 返回元数据 JSON 指针         |
| `ayiou_matches` | `(ctx_ptr, ctx_len) -> i32` | 检查是否匹配 (1=是, 0=否)    |
| `ayiou_handle`  | `(ctx_ptr, ctx_len) -> i32` | 处理事件，返回响应 JSON 指针 |
| `ayiou_alloc`   | `(size) -> i32`             | 分配内存                     |
| `ayiou_free`    | `(ptr)`                     | 释放内存                     |

### 数据格式

**元数据 (ayiou_meta 返回):**
```json
{
  "name": "plugin-name",
  "description": "Plugin description",
  "version": "1.0.0"
}
```

**上下文 (传入 ayiou_matches/ayiou_handle):**
```json
{
  "text": "消息文本",
  "raw_message": "原始消息",
  "user_id": 123456,
  "group_id": 789012,
  "is_private": false,
  "is_group": true,
  "nickname": "用户昵称"
}
```

**响应 (ayiou_handle 返回):**
```json
{
  "block": true,
  "reply": "回复消息"
}
```

### 内存协议

字符串使用长度前缀格式：
- 前 4 字节：字符串长度 (little-endian u32)
- 后续字节：UTF-8 编码的字符串内容

## 开发提示

1. AssemblyScript 类型与 TypeScript 略有不同，使用 `i32`, `i64`, `f32`, `f64` 等原生类型
2. 使用 `memory.data()` 分配静态内存
3. 使用 `String.UTF8.encode/decode` 处理字符串
4. IDE 可能会报类型错误，这是因为 AssemblyScript 有自己的类型定义，安装依赖后会自动解决
