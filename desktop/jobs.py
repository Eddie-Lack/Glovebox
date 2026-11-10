"""Runs slow back-end calls (the model catalog, ROM builds, model imports) off the window's thread."""
import traceback
from concurrent.futures import ThreadPoolExecutor
from PySide6.QtCore import QObject, Signal


class Jobs(QObject):
    _done = Signal(object, object, object)   # callback, result, error

    def __init__(self, workers=2):
        super().__init__()
        self.pool = ThreadPoolExecutor(workers)
        self._done.connect(self._deliver)

    def run(self, fn, on_done=None, on_error=None):
        """Calls fn() in a worker thread, then on_done(result) or on_error(exception) in the window's thread"""
        def work():
            try:
                self._done.emit((on_done, on_error), fn(), None)
            except Exception as e:  # handed to on_error in the window's thread
                traceback.print_exc()
                self._done.emit((on_done, on_error), None, e)
        self.pool.submit(work)

    def _deliver(self, cbs, result, error):
        on_done, on_error = cbs
        if error is not None:
            if on_error:
                on_error(error)
        elif on_done:
            on_done(result)
