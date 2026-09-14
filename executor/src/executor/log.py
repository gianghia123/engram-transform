import logging
import sqlite3 as sql
from dataclasses import dataclass

@dataclass
class Trace:
    job_id: str
    program_hash: str
    input_digest: str
    params_digest: str
    fuel_consumed: int
    output_digest: str
    trap: int

class DatabaseHandler(logging.Handler):
    def __init__(self, path):
        super().__init__()
        self._db = sql.connect(path)
        self._db.execute(
            "CREATE TABLE IF NOT EXISTS traces ("
            "job_id TEXT, program_hash TEXT, input_digest TEXT, "
            "params_digest TEXT, fuel_consumed INTEGER, output_digest TEXT, "
            "trap INTEGER)"
        )
        self._db.commit()

    def emit(self, record):
        trace = record.msg
        if not isinstance(trace, Trace):
            return

        self._db.execute(
            "INSERT INTO traces VALUES (?, ?, ?, ?, ?, ?, ?)",
            (trace.job_id, trace.program_hash, trace.input_digest,
             trace.params_digest, trace.fuel_consumed, trace.output_digest,
             trace.trap),
        )
        self._db.commit()
