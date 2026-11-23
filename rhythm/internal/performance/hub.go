package performance

import "sync"

// subscriberBuffer is how many frames a slow subscriber may lag behind before
// its oldest frames are dropped.
const subscriberBuffer = 8

// Hub fans encoded frames out to subscribers. Publishing never blocks: a
// subscriber whose buffer is full loses its oldest frame, so a slow client
// can never slow down a beat.
type Hub struct {
	mu     sync.Mutex
	subs   map[chan []byte]struct{}
	last   []byte
	closed bool
}

// NewHub returns an empty hub.
func NewHub() *Hub {
	return &Hub{subs: make(map[chan []byte]struct{})}
}

// Subscribe returns a channel of frames, starting with the latest one if any,
// and a function that unsubscribes. The channel is closed when the hub closes
// or the subscriber unsubscribes.
func (h *Hub) Subscribe() (<-chan []byte, func()) {
	ch := make(chan []byte, subscriberBuffer)
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.closed {
		close(ch)
		return ch, func() {}
	}
	if h.last != nil {
		ch <- h.last
	}
	h.subs[ch] = struct{}{}
	return ch, func() {
		h.mu.Lock()
		defer h.mu.Unlock()
		if _, ok := h.subs[ch]; ok {
			delete(h.subs, ch)
			close(ch)
		}
	}
}

// Publish sends frame to every subscriber, dropping each full subscriber's
// oldest frame to make room.
func (h *Hub) Publish(frame []byte) {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.closed {
		return
	}
	h.last = frame
	for ch := range h.subs {
		select {
		case ch <- frame:
			continue
		default:
		}
		// Full: drop the oldest frame, then send. Only Publish sends, and it
		// holds the lock, so the second send cannot block.
		select {
		case <-ch:
		default:
		}
		ch <- frame
	}
}

// Count returns the number of current subscribers.
func (h *Hub) Count() int {
	h.mu.Lock()
	defer h.mu.Unlock()
	return len(h.subs)
}

// Close closes every subscriber channel; later subscribers get a closed channel.
func (h *Hub) Close() {
	h.mu.Lock()
	defer h.mu.Unlock()
	if h.closed {
		return
	}
	h.closed = true
	for ch := range h.subs {
		close(ch)
	}
	clear(h.subs)
}
