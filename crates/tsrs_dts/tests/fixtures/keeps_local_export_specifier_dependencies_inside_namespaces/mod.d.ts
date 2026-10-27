declare const Referenced: { value: string };
declare namespace Ns {
  export { Referenced as Ref };
}
export { Ns };
