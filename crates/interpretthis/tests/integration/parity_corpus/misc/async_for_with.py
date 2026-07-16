import asyncio


class AsyncRange:
    def __init__(self, n):
        self.n = n
        self.i = 0

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self.i >= self.n:
            raise StopAsyncIteration
        self.i += 1
        return self.i - 1


async def use_async_for():
    result = []
    async for x in AsyncRange(4):
        result.append(x)
    return result


print(asyncio.run(use_async_for()))


class AsyncCM:
    def __init__(self, name):
        self.name = name

    async def __aenter__(self):
        print(f"enter {self.name}")
        return self.name

    async def __aexit__(self, *a):
        print(f"exit {self.name}")
        return False


async def use_async_with():
    async with AsyncCM("A") as a:
        print(f"body {a}")
    return "done"


print(asyncio.run(use_async_with()))


async def async_for_break():
    total = 0
    async for x in AsyncRange(10):
        if x >= 3:
            break
        total += x
    return total


print(asyncio.run(async_for_break()))


async def nested_async_with():
    async with AsyncCM("X") as x, AsyncCM("Y") as y:
        print(f"both {x} {y}")
    return "nested done"


print(asyncio.run(nested_async_with()))


async def async_with_exception():
    try:
        async with AsyncCM("E"):
            raise ValueError("boom")
    except ValueError as e:
        return f"caught {e}"


print(asyncio.run(async_with_exception()))


async def async_for_else():
    async for x in AsyncRange(2):
        pass
    else:
        return "else ran"


print(asyncio.run(async_for_else()))
