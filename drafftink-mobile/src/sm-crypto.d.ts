// sm-crypto 未提供官方类型声明，这里做最小化的模块声明。
declare module 'sm-crypto' {
  export const sm4: {
    encrypt(
      msg: string | number[],
      key: string | number[],
      options?: { mode?: string; padding?: string; iv?: string; input?: string; output?: string },
    ): string | number[];
    decrypt(
      msg: string | number[],
      key: string | number[],
      options?: { mode?: string; padding?: string; iv?: string; input?: string; output?: string },
    ): string | number[];
  };
}
