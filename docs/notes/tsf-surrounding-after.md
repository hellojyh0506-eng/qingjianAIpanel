# TSF 读取光标后文：部署与回滚（P5）

## 改了什么

- `qingjian_tsf.dll`：起组句时除光标前文（64 字）外，再读光标后文（32 字），一起放进 `Surrounding` 消息。私密输入框不读。
- `qingjian-server.exe`：收下 `after`，交给云端 Resolve 判断「这一串想打什么」；Surrounding 迟到时用新前后文重发 Resolve。
- 协议只多了一个可选字段 `after`，`PROTOCOL_VERSION` 仍是 7。

## 互通矩阵

| DLL | Server | 结果 |
|---|---|---|
| 旧 | 新 | 能用。旧 DLL 不带 `after`，Server 按空处理，行为和 P4 一样（只有前文）。 |
| 新 | 旧 | 能用。旧 Server 的 `Surrounding` 只认 `session` / `text`，多出来的 `after` 被忽略。 |
| 新 | 新 | 前后文都给 Resolve。 |

所以 Server 与 DLL 可以分开、按任意顺序更新，不需要同时换。

## 部署

1. 先换 Server：退出 `qingjian-server.exe`，替换后重新启动（旧 DLL 照常工作）。
2. 再换 DLL：DLL 被各应用进程占着，直接覆盖会失败。做法与发布流程一致：注销旧 DLL、替换文件（或用安装包），重新登录或重启受影响的应用后生效。
3. **回滚**：保留旧的 `qingjian_tsf.dll`（换之前复制一份，例如 `qingjian_tsf.dll.prev`）。出问题时按上面的步骤换回旧 DLL 即可，Server 不用动，因为旧 DLL 与新 Server 互通。

## 验收（需要在 Windows 真机做）

在记事本里把光标放进一句话中间，敲一串拼音，看 DLL 日志里有没有「送光标前后文 前 N 字 后 M 字」且 M > 0。日志只记字数，不记内容。
密码框 / 无痕窗口里应该看到「私密输入框，不读光标前后文」，且没有送出前后文。

## 已知限制

- 有的宿主控件（部分 Electron / 游戏 / 远程桌面）读不到文本，`ShiftEnd` 返回 0，此时 `after` 为空，行为退回只有前文。
- 光标后文只在起组句那一刻读一次；组句过程中应用里文字变了不会更新。
