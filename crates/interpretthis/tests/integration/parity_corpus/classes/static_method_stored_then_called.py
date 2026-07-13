# Pins: `f = Cls.static_method; f(arg)` — store class method then call.
# eval_call variable-lookup branch sees a __class_method__ sentinel; no arm
# today; errors "'f' is not callable".
class Cls:
    @staticmethod
    def double(x):
        return x * 2
f = Cls.double
print(f(21))
