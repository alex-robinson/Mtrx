package org.mtrx.app.model

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import androidx.datastore.preferences.preferencesDataStore
import java.io.IOException
import kotlinx.coroutines.flow.first

/** What the app shows, kept across launches (as the desktop's `view.json`). */
val Context.viewPreferences: DataStore<Preferences> by preferencesDataStore(name = "view")

/** The selected space in [preferences]. */
class DataStoreSelectedSpace(private val preferences: DataStore<Preferences>) : SelectedSpaceStore {
    override suspend fun load(): SpaceEntry? = try {
        preferences.data.first()[SELECTED_SPACE]?.let { SpaceEntry.fromKey(it) }
    } catch (e: IOException) {
        null
    }

    override suspend fun save(entry: SpaceEntry?) {
        try {
            preferences.edit {
                it.remove(SELECTED_SPACE)
                if (entry != null) it[SELECTED_SPACE] = entry.key
            }
        } catch (e: IOException) {
            // Not remembered; the app works without it.
        }
    }

    private companion object {
        val SELECTED_SPACE = stringPreferencesKey("selected_space")
    }
}

/** The order of the spaces' rooms in [preferences]. */
class DataStoreRoomSort(private val preferences: DataStore<Preferences>) : RoomSortStore {
    override suspend fun load(): RoomSort? = try {
        preferences.data.first()[ROOM_SORT]?.let { saved -> RoomSort.entries.firstOrNull { it.name == saved } }
    } catch (e: IOException) {
        null
    }

    override suspend fun save(sort: RoomSort) {
        try {
            preferences.edit { it[ROOM_SORT] = sort.name }
        } catch (e: IOException) {
            // Not remembered; the app works without it.
        }
    }

    private companion object {
        val ROOM_SORT = stringPreferencesKey("room_sort")
    }
}

/** "Share my presence" in [preferences]. */
class DataStoreSharePresence(private val preferences: DataStore<Preferences>) : SharePresenceStore {
    override suspend fun load(): Boolean? = try {
        preferences.data.first()[SHARE_PRESENCE]
    } catch (e: IOException) {
        null
    }

    override suspend fun save(on: Boolean) {
        try {
            preferences.edit { it[SHARE_PRESENCE] = on }
        } catch (e: IOException) {
            // Not remembered; the default (on) comes back at the next launch.
        }
    }

    private companion object {
        val SHARE_PRESENCE = booleanPreferencesKey("share_presence")
    }
}
