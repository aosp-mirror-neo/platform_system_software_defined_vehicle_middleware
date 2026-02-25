/*
 * Copyright (C) 2026 The Android Open Source Project
 *
 * Licensed under the Apache License, Version 2.0 (the "License");
 * you may not use this file except in compliance with the License.
 * You may obtain a copy of the License at
 *
 *      http://www.apache.org/licenses/LICENSE-2.0
 *
 * Unless required by applicable law or agreed to in writing, software
 * distributed under the License is distributed on an "AS IS" BASIS,
 * WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
 * See the License for the specific language governing permissions and
 * limitations under the License.
 */

#pragma once

#include <algorithm>
#include <string>
#include <vector>

class RawBuffer {
public:
    RawBuffer(size_t size = 0) : size_(size) {
        if (size) ptr_ = new uint8_t[size];
    }

    RawBuffer(const std::vector<uint8_t>& v) : RawBuffer(v.size()) {
        if (size_) {
            memcpy(ptr_, v.data(), size_);
        }
    }

    RawBuffer(RawBuffer&& other) :
          size_(std::exchange(other.size_, 0)), ptr_(std::exchange(other.ptr_, nullptr)) {}

    RawBuffer& operator=(RawBuffer&& other) noexcept {
        if (this != &other) {
            size_ = std::exchange(other.size_, 0);
            ptr_ = std::exchange(other.ptr_, nullptr);
        }
        return *this;
    }

    ~RawBuffer() {
        if (ptr_) delete[] ptr_;
        ptr_ = nullptr;
        size_ = 0;
    }

    RawBuffer(const RawBuffer&) = delete;
    RawBuffer& operator=(const RawBuffer&) = delete;

    const uint8_t* Ptr() const { return ptr_; }
    uint8_t* Ptr() { return ptr_; }
    size_t Size() const { return size_; }

    bool ToVector(std::vector<uint8_t>& v) const {
        if (!size_ || !ptr_) return false;
        v.resize(size_);
        memcpy(v.data(), ptr_, size_);
        return true;
    }

    void Resize(size_t new_size, bool copy_data) {
        if (new_size != size_) {
            size_t to_copy = std::min(new_size, size_);
            size_ = new_size;
            uint8_t* old_ptr = ptr_;
            if (new_size)
                ptr_ = new uint8_t[size_];
            else
                ptr_ = nullptr;

            if (old_ptr) {
                if (ptr_ && copy_data && to_copy) {
                    memcpy(ptr_, old_ptr, to_copy);
                }
                delete[] old_ptr;
            }
        }
    }

private:
    size_t size_ = 0;
    uint8_t* ptr_ = nullptr;
};

class BufferPosTracker {
public:
    BufferPosTracker(RawBuffer& buffer, size_t pos = 0) : buffer_(buffer), pos_(pos) {}
    BufferPosTracker(const BufferPosTracker& other) : buffer_(other.buffer_), pos_(other.pos_) {}
    size_t GetPos() const { return pos_; }
    size_t SetPos(size_t pos) {
        pos_ = std::min(pos, buffer_.Size());
        return pos_;
    }
    void Reset() { pos_ = 0; }
    size_t MovePos(size_t offset) { return SetPos(pos_ + offset); }
    size_t Available() const { return (buffer_.Size() < pos_) ? 0 : (buffer_.Size() - pos_); }
    const uint8_t* Ptr() const { return buffer_.Ptr(); }
    uint8_t* Ptr() { return buffer_.Ptr(); }
    const uint8_t* CurPtr() const { return buffer_.Ptr() + pos_; }
    uint8_t* CurPtr() { return buffer_.Ptr() + pos_; }
    RawBuffer& Buffer() { return buffer_; }
    const RawBuffer& Buffer() const { return buffer_; }

    void Inflate(size_t inflate_by) { buffer_.Resize(buffer_.Size() + inflate_by, true); }

private:
    RawBuffer& buffer_;
    size_t pos_ = 0;
};

/**
 * Simplified version of data serialization for transport over vsock,
 * i.e. assuming we do both serialization/deserialization on the same
 * CPU, i.e. with the same byte order.
 */
class BufferWriter : public BufferPosTracker {
public:
    BufferWriter(RawBuffer& buffer) : BufferPosTracker(buffer) {}

    bool Write(const void* buf, uint32_t buf_size) {
        if (Available() < buf_size) return false;
        memcpy(CurPtr(), buf, buf_size);
        MovePos(buf_size);
        return true;
    }

    // TODO: Add casting with alignment checking
    template <class T>
    static bool CastToBuffer(BufferWriter& bw, T v) {
        if (bw.Available() < sizeof(T)) return false;
        memcpy(bw.CurPtr(), &v, sizeof(T));
        bw.MovePos(sizeof(T));
        return true;
    }

    bool Write(uint32_t v) { return CastToBuffer<uint32_t>(*this, v); }
    bool Write(int32_t v) { return CastToBuffer<int32_t>(*this, v); }
    bool Write(uint16_t v) { return CastToBuffer<uint16_t>(*this, v); }
    bool Write(int16_t v) { return CastToBuffer<int16_t>(*this, v); }
    bool Write(uint8_t v) { return CastToBuffer<uint8_t>(*this, v); }
    bool Write(int8_t v) { return CastToBuffer<int8_t>(*this, v); }

    bool Write(const std::string& v) {
        return Available() >= (sizeof(uint16_t) + v.length()) &&
                Write(static_cast<uint16_t>(v.length())) && Write(v.c_str(), v.length());
    }

    size_t CutAndMove(size_t to_cut) {
        to_cut = std::min(to_cut, GetPos());
        if (to_cut < GetPos()) {
            size_t left = GetPos() - to_cut;
            memmove(Ptr(), Ptr() + to_cut, left);
            SetPos(left);
        } else
            Reset();
        return GetPos();
    }
};

class BufferReader : public BufferPosTracker {
public:
    BufferReader(RawBuffer& buffer, uint32_t available_to_read) :
          BufferPosTracker(buffer), available_to_read_(available_to_read) {}

    BufferReader(const BufferReader& other) :
          BufferPosTracker(other), available_to_read_(other.available_to_read_) {}

    BufferReader(const BufferWriter& writer) :
          BufferPosTracker(const_cast<BufferWriter&>(writer).Buffer()),
          available_to_read_(writer.GetPos()) {}

    uint32_t AvailableToRead() const {
        return (available_to_read_ < GetPos()) ? 0 : (available_to_read_ - GetPos());
    }

    uint32_t SetAvailableToRead(uint32_t available_to_read) {
        available_to_read_ = available_to_read;
        return available_to_read_;
    }

    bool Read(void* buf, uint32_t buf_size) {
        if (AvailableToRead() < buf_size) return false;
        memcpy(buf, CurPtr(), buf_size);
        MovePos(buf_size);
        return true;
    }

    // TODO: Add casting with alignment checking
    template <class T>
    static bool CastFromBuffer(BufferReader& br, T& v) {
        if (br.AvailableToRead() < sizeof(T)) return false;
        memcpy(&v, br.CurPtr(), sizeof(T));
        br.MovePos(sizeof(T));
        return true;
    }

    bool Read(uint32_t& v) { return CastFromBuffer<uint32_t>(*this, v); }
    bool Read(int32_t& v) { return CastFromBuffer<int32_t>(*this, v); }
    bool Read(uint16_t& v) { return CastFromBuffer<uint16_t>(*this, v); }
    bool Read(int16_t& v) { return CastFromBuffer<int16_t>(*this, v); }
    bool Read(uint8_t& v) { return CastFromBuffer<uint8_t>(*this, v); }
    bool Read(int8_t& v) { return CastFromBuffer<int8_t>(*this, v); }

    bool Read(std::string& v) {
        // assuming string is array of chars
        uint16_t len;
        Read(len);
        std::vector<char> chars(len);
        if (!Read(&chars[0], len)) return false;
        v = std::string(chars.begin(), chars.end());
        return true;
    }

private:
    uint32_t available_to_read_;
};
