export {};
class Café {
  fresh(): Café { return new Café(); }
  outer(): Café {
    class Inner { method(): Inner { return this; } }
    return this;
  }
  tryReturn(): Café | undefined {
    try { return this; } catch { return undefined; }
  }
  parameter<T extends this>(value: T): Café { return value; }
  parenthesizedThis(): Café { return (this); }
  none(): Café { return; }
}
