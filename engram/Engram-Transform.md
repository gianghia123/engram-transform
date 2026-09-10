---
up:
tags:
  - project
status: ongoing
---
## Description

A data transformation engine for Engram.

#### Vision

A deterministic, cryptography-provable data transformation engine.

### Research questions

RQ1. Làm thế nào ràng buộc chương trình, tham số và kết quả với đúng Engram object/version?

RQ2. Làm thế nào chứng minh toàn bộ partition plan đã được xử lý, không bỏ sót và không lặp chunk?

RQ3. Khi một phần đầu vào thay đổi, proof và output nào có thể tái sử dụng an toàn?

RQ4. Chi phí prover, network và storage thay đổi thế nào theo chunk size, program complexity và tỷ lệ cập nhật?

### MVP

- Chương trình deterministic, chạy trên dữ liệu chunked; không truy cập clock, network hoặc randomness ngoài job manifest.
- Phép tính số nguyên/fixed-point đơn giản: filter, count, sum, histogram, checksum và chuẩn hóa.
- Một coordinator tạo partition plan; coordinator có thể Byzantine trong giai đoạn đánh giá completeness.
- Một zkVM backend duy nhất sau vòng benchmark; không xây hai prover stack song song.

Tech stack:

[] Rust + Wasmtime (or even Python)

---

### Checklist

[] Previous works?
[] Write the single-node executor.
[] Proving *shii*.

---

### Related notes
