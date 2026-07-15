class A:
    @classmethod
    def create(cls, x):
        return f"A.create({x})"
class B(A):
    @classmethod
    def create(cls, x):
        base = super().create(x)
        return f"B->{base}"
class C(B):
    @classmethod
    def create(cls, x):
        base = super().create(x)
        return f"C->{base}"
print(C.create(5))
print(B.create(3))

registry = []
class Plugin:
    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        registry.append(cls.__name__)
class PluginA(Plugin):
    pass
class PluginB(Plugin):
    pass
print(registry)

class Config:
    settings = {}
    def __init_subclass__(cls, prefix="", **kwargs):
        super().__init_subclass__(**kwargs)
        cls.prefix = prefix
class MyConfig(Config, prefix="my"):
    pass
print(MyConfig.prefix)
