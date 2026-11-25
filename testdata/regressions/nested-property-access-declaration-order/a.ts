class C {
  static self: typeof C = C;
  static value = (C.self.later = 1);
  static readonly later = C.value;
}
class F {
  static first = F.second;
  static nested = F.self.second;
  static self: typeof F = F;
  static second = 1;
}
