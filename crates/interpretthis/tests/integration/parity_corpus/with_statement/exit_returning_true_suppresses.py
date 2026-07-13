# Pins: __exit__ returning a truthy value suppresses the exception.
# CPython protocol: if __exit__ returns True, the raised exception
# is silently swallowed; control flows past the with block.
class Suppressor:
    def __enter__(self):
        return self
    def __exit__(self, exc_type, exc_value, traceback):
        return True

with Suppressor():
    raise ValueError('this gets suppressed')
print('after — suppression worked')
