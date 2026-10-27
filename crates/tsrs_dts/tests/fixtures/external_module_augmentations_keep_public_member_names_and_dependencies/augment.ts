import 'external'; interface Private { value: string } declare module 'external' { interface Item { extra: Private } }
