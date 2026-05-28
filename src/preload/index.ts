import { contextBridge, ipcRenderer } from 'electron';

contextBridge.exposeInMainWorld('api', {
  send(command: string, data?: unknown): void {
    ipcRenderer.send('from-renderer', { command, data });
  },
  onMessage(callback: (msg: { command: string; data: unknown }) => void): void {
    ipcRenderer.on('to-renderer', (_event, msg) => callback(msg));
  },
  invoke(command: string, data?: unknown): Promise<unknown> {
    return ipcRenderer.invoke('ipc-invoke', { command, data });
  },
  winMin(): void { ipcRenderer.send('win-ctrl', 'min'); },
  winMax(): void { ipcRenderer.send('win-ctrl', 'max'); },
  winClose(): void { ipcRenderer.send('win-ctrl', 'close'); },
  onWinMaximize(cb: (maximized: boolean) => void): void {
    ipcRenderer.on('win-maximized', (_e, v) => cb(v as boolean));
  },
});
