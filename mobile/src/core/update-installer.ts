export type InstallUpdateResult = {
  supported: boolean;
  message: string;
};

export async function installUpdateFromUri(
  _uri: string,
): Promise<InstallUpdateResult> {
  return {
    supported: false,
    message: "移动端更新安装暂不在 MVP 中启用",
  };
}
