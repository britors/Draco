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
});
