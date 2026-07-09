# Pins: parametric decorator — decorator factory returns a decorator
# that wraps the function. Common pattern: @retry(3), @cache(ttl=60).
def repeat(n):
    def deco(func):
        def wrapper(*args, **kwargs):
            return [func(*args, **kwargs) for _ in range(n)]
        return wrapper
    return deco

@repeat(3)
def greet(name):
    return f"hi {name}"

print(greet("alice"))
