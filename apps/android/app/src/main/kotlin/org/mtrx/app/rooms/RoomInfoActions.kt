package org.mtrx.app.rooms

import uniffi.mtrx.Access
import uniffi.mtrx.History
import uniffi.mtrx.NotifyMode
import uniffi.mtrx.Permissions
import uniffi.mtrx.Topic

/**
 * The changes Room info makes ([RoomInfoModel]); the screenshot tests give
 * one that does nothing. People by user ID, spaces by room ID.
 */
interface RoomInfoActions {
    fun rename(name: String)

    fun setTopic(topic: Topic)

    fun setNotify(mode: NotifyMode)

    fun setAccess(access: Access)

    fun setListed(listed: Boolean)

    fun setSuggested(spaceId: String, suggested: Boolean)

    fun setHistory(history: History)

    fun setPermissions(permissions: Permissions)

    fun encrypt()

    fun setAddress(name: String)

    fun addToSpace(spaceId: String)

    fun removeFromSpace(spaceId: String)

    fun unban(userId: String)

    fun setLevel(userId: String, level: Long)

    fun remove(userId: String, reason: String, ban: Boolean)

    fun invite(userId: String)

    fun clearError()
}
