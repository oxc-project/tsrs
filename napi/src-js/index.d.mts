import { API as SubprocessAPI } from "tsrs/sync";

export * from "tsrs/sync";
export declare const API: typeof SubprocessAPI;
export type API<FromLSP extends boolean = false> = SubprocessAPI<FromLSP>;
