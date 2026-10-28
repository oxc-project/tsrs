import 'external'; interface Private { value: string } declare global { interface GlobalItem { private: Private } }
