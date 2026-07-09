# @staticmethod: called via instance with no self binding. Pins
# instance_method_call's lookup_static_method early-return path.
class Math:
    @staticmethod
    def add(a, b):
        return a + b

    @staticmethod
    def square(x):
        return x * x

print(Math.add(2, 3))           # 5 — called via class
m = Math()
print(m.add(10, 20))            # 30 — called via instance, no self
print(m.square(7))              # 49
