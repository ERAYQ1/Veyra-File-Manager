# Veyra — Performance Budget & Benchmarks

**Document Status:** Approved Architecture  
**Target:** Dolphin-Level Performance & Instant UI Responsiveness

---

## 1. Quantitative Performance Budgets

| Metric | Target Budget | Upper Acceptable Limit |
| :--- | :--- | :--- |
| **Cold Startup Time** | **< 350 ms** | < 800 ms |
| **Warm Startup Time** | **< 150 ms** | < 300 ms |
| **Idle Base RAM Footprint** | **< 50 MB** | < 85 MB |
| **RAM Footprint (100k Files Directory)** | **< 180 MB** | < 250 MB |
| **Directory Read Latency (1,000 files)** | **< 15 ms** | < 40 ms |
| **Directory Read Latency (100,000 files)** | **< 200 ms (Incremental)** | < 500 ms (First Batch Visible < 50ms) |
| **UI Frame Rate** | **60 - 120 FPS** | Constant 60 FPS (Zero Stutters) |
| **GTK Main Thread Block Time** | **0 ms** | < 16 ms (Single Frame Limit) |

---

## 2. Directory Scalability Engine

```
Directory Entry Count Benchmark Thresholds:
├── 100 entries       : Immediate sync/async read (< 2ms)
├── 1,000 entries     : Single batch list read (< 15ms)
├── 10,000 entries    : Chunked async streaming (500 entry batches)
└── 100,000+ entries  : Incremental virtualized loading with lazy metadata
```

### Virtualization & Incremental Strategy
1. **Streaming Batches:** When reading massive directories, `veyra-filesystem` emits chunks of 500 items over async channels. The UI updates the visible scroll range immediately without waiting for the full folder enumeration.
2. **Lazy Metadata Fetching:** Basic stat info (Name, Size, Type) is fetched first. Detailed metadata (extended attributes, MIME deep inspection, checksums, thumbnails) is loaded lazily only for entries in the visible viewport.

---

## 3. Background Task Resource Management

- **IO Priority Throttling:** Background search indexers, thumbnail generation workers, and checksum calculators must execute under `SCHED_IDLE` or `nice +19` CPU scheduling and `IOPRIO_CLASS_IDLE` IO scheduling.
- **Memory Throttling:** Thumbnail cache enforces strict LRU eviction capped at a maximum configurable limit (default: 100MB RAM, 1GB Disk).
